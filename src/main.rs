//! UtaTUI — a terminal Now Playing dashboard for any MPRIS media player.

mod app;
mod config;
mod media;
mod mpris;
mod ui;
mod visualizer;

use std::path::PathBuf;
use std::time::Duration;

use ratatui::crossterm::event;
use ratatui_image::picker::{Picker, ProtocolType};
use tokio::sync::mpsc::{self, UnboundedReceiver};

use app::{App, Event};
use config::Config;

const USAGE: &str = "\
utatui — terminal media controller for MPRIS players

USAGE: utatui [--config <path>] [--list]

  --config <path>  use this config file instead of ~/.config/utatui/config.toml
  --list           print detected players and exit
  --help           show this help
  --version        show version";

// I/O-bound: one thread is plenty, and image work already runs on the blocking pool.
#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("utatui: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let (mut config_path, mut list) = (None, false);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--config" => {
                config_path = Some(PathBuf::from(args.next().ok_or("--config needs a path")?));
            }
            "--list" => list = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            "-V" | "--version" => {
                println!("utatui {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            other => return Err(format!("unknown argument `{other}`\n\n{USAGE}")),
        }
    }
    let cfg = Config::load(config_path.clone())?;

    let (tx, rx) = mpsc::unbounded_channel();
    let conn = zbus::Connection::session().await;
    if let Ok(conn) = &conn {
        let (conn, tx) = (conn.clone(), tx.clone());
        tokio::spawn(async move {
            if let Err(e) = mpris::watch(conn, tx.clone()).await {
                let _ = tx.send(Event::Status(format!("MPRIS watcher stopped: {e}")));
            }
        });
    }
    if list {
        return list_players(conn.map_err(|e| e.to_string())?, rx).await;
    }
    let conn = conn.ok();

    let picker = make_picker(&cfg.artwork.protocol)?;

    // Image resizing/encoding is CPU work; keep it off the render loop.
    let (resize_tx, mut resize_rx) = mpsc::unbounded_channel::<ratatui_image::thread::ResizeRequest>();
    let art_tx = tx.clone();
    tokio::spawn(async move {
        while let Some(req) = resize_rx.recv().await {
            let tx = art_tx.clone();
            tokio::task::spawn_blocking(move || {
                if let Ok(res) = req.resize_encode() {
                    let _ = tx.send(Event::Resized(res));
                }
            });
        }
    });

    // crossterm's reader is blocking; give it a thread of its own.
    let input_tx = tx.clone();
    std::thread::spawn(move || {
        while let Ok(ev) = event::read() {
            if input_tx.send(Event::Term(ev)).is_err() {
                break;
            }
        }
    });

    let mut terminal = ratatui::init();
    let mut app = App::new(cfg, config_path, conn, tx, picker, resize_tx);
    let res = event_loop(&mut terminal, &mut app, rx).await;
    ratatui::restore();
    res
}

/// Must run before the TUI owns stdin: "auto" queries the terminal for graphics support.
fn make_picker(protocol: &str) -> Result<Picker, String> {
    let forced = match protocol {
        "auto" => None,
        "halfblocks" => return Ok(Picker::halfblocks()),
        "kitty" => Some(ProtocolType::Kitty),
        "sixel" => Some(ProtocolType::Sixel),
        "iterm2" => Some(ProtocolType::Iterm2),
        other => return Err(format!("unknown artwork.protocol `{other}`")),
    };
    let mut picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
    if let Some(p) = forced {
        picker.set_protocol_type(p);
    }
    Ok(picker)
}

async fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    mut rx: UnboundedReceiver<Event>,
) -> Result<(), String> {
    loop {
        terminal.draw(|f| ui::draw(f, app)).map_err(|e| e.to_string())?;
        let tick = app.tick_rate();
        tokio::select! {
            ev = rx.recv() => match ev {
                Some(ev) => app.handle(ev),
                None => return Ok(()),
            },
            () = tokio::time::sleep(tick.unwrap_or_default()), if tick.is_some() => app.on_tick(),
        }
        // Coalesce bursts (e.g. 60 fps spectrum + key repeat) into a single redraw.
        while let Ok(ev) = rx.try_recv() {
            app.handle(ev);
        }
        if app.quit {
            return Ok(());
        }
    }
}

/// `--list`: waits briefly for discovery, then prints what was found.
async fn list_players(_conn: zbus::Connection, mut rx: UnboundedReceiver<Event>) -> Result<(), String> {
    let mut players = Vec::new();
    let deadline = tokio::time::sleep(Duration::from_millis(1500));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            () = &mut deadline => break,
            Some(ev) = rx.recv() => if let Event::Player(mpris::Update::Changed(p)) = ev {
                players.retain(|q: &mpris::Player| q.bus != p.bus);
                players.push(p);
            },
        }
    }
    if players.is_empty() {
        println!("no MPRIS players found");
    }
    for p in players {
        println!(
            "{} ({})\n  status: {:?}  position: {}/{}\n  title: {}\n  artist: {}\n  album: {}\n  art: {}\n  caps: {:?}\n",
            p.identity,
            p.bus,
            p.status,
            media::fmt_time(p.position_now()),
            p.length.map_or("?".into(), media::fmt_time),
            p.title,
            p.artists.join(", "),
            p.album,
            p.art_url.as_deref().unwrap_or("-"),
            p.caps,
        );
    }
    Ok(())
}
