//! Application state and event handling. All I/O happens in background tasks that report
//! back through `Event`s, so handling an event never blocks the render loop.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use image::DynamicImage;
use ratatui::crossterm::event::{Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::style::Color;
use ratatui_image::picker::Picker;
use ratatui_image::thread::{ResizeRequest, ResizeResponse, ThreadProtocol};
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;
use zbus::Connection;

use crate::config::settings::{self, Effect, Field};
use crate::config::{Config, KeyAction, Mode};
use crate::mpris::{self, Action, Player, Status, Update};
use crate::{media, visualizer};

/// How often the selected player's position is re-read while playing.
const POSITION_RESYNC: Duration = Duration::from_secs(3);
/// Delay before restarting cava after a setting change, so holding a key restarts it once.
const VIZ_RESTART_DEBOUNCE: Duration = Duration::from_millis(400);

pub enum Event {
    Term(TermEvent),
    Player(Update),
    Art { url: String, img: Option<DynamicImage>, accent: Option<Color> },
    Resized(ResizeResponse),
    Spectrum(Vec<f32>),
    VisualizerDown(String),
    Status(String),
}

pub enum Overlay {
    None,
    Selector(usize),
    Help,
    /// Selected field index, and the text being typed when editing a text field.
    Settings {
        sel: usize,
        edit: Option<String>,
    },
}

pub struct App {
    pub cfg: Config,
    config_path: Option<PathBuf>,
    /// Settings changed since the last save.
    pub dirty: bool,
    conn: Option<Connection>,
    tx: UnboundedSender<Event>,
    pub players: Vec<Player>,
    selected: Option<String>,
    /// Set when the user picked a player by hand; disables automatic switching.
    pinned: bool,
    art_url: Option<String>,
    pub art: Option<ThreadProtocol>,
    pub art_accent: Option<Color>,
    picker: Picker,
    resize_tx: UnboundedSender<ResizeRequest>,
    viz_task: Option<JoinHandle<()>>,
    viz_restart_at: Option<Instant>,
    pub spectrum: Vec<f32>,
    /// `None` while cava delivers audio; otherwise why the idle animation is shown.
    pub viz_fallback: Option<String>,
    pub overlay: Overlay,
    status: Option<(String, Instant)>,
    last_resync: Instant,
    started: Instant,
    pub quit: bool,
}

impl App {
    pub fn new(
        cfg: Config,
        config_path: Option<PathBuf>,
        conn: Option<Connection>,
        tx: UnboundedSender<Event>,
        picker: Picker,
        resize_tx: UnboundedSender<ResizeRequest>,
    ) -> Self {
        let status = conn.is_none().then(|| ("D-Bus session bus unavailable".to_string(), Instant::now()));
        let mut app = Self {
            cfg,
            config_path,
            dirty: false,
            conn,
            tx,
            players: Vec::new(),
            selected: None,
            pinned: false,
            art_url: None,
            art: None,
            art_accent: None,
            picker,
            resize_tx,
            viz_task: None,
            viz_restart_at: None,
            spectrum: Vec::new(),
            viz_fallback: None,
            overlay: Overlay::None,
            status,
            last_resync: Instant::now(),
            started: Instant::now(),
            quit: false,
        };
        app.restart_visualizer();
        app
    }

    pub fn current(&self) -> Option<&Player> {
        let bus = self.selected.as_ref()?;
        self.players.iter().find(|p| &p.bus == bus)
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_ref().map(|(s, _)| s.as_str())
    }

    fn set_status(&mut self, s: impl Into<String>) {
        self.status = Some((s.into(), Instant::now()));
    }

    /// The accent in effect: from the artwork when enabled and available, else the theme's.
    pub fn accent(&self) -> Color {
        match self.art_accent {
            Some(c) if self.cfg.theme.accent_from_artwork => c,
            _ => crate::config::color(&self.cfg.theme.accent),
        }
    }

    fn playing(&self) -> bool {
        self.current().is_some_and(|p| p.status == Status::Playing)
    }

    /// How long the loop may sleep before the screen needs refreshing on its own.
    pub fn tick_rate(&self) -> Option<Duration> {
        let playing = self.playing();
        if self.viz_restart_at.is_some() {
            Some(Duration::from_millis(100))
        } else if playing && self.viz_fallback.is_some() && self.cfg.visualizer.enabled {
            Some(Duration::from_millis(50)) // idle animation
        } else if playing || self.status.is_some() {
            Some(Duration::from_millis(500)) // progress bar / status expiry
        } else {
            None
        }
    }

    pub fn on_tick(&mut self) {
        if self.status.as_ref().is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(4)) {
            self.status = None;
        }
        if self.viz_restart_at.is_some_and(|at| at.elapsed() >= VIZ_RESTART_DEBOUNCE) {
            self.restart_visualizer();
        }
        if self.playing() && self.last_resync.elapsed() >= POSITION_RESYNC {
            self.last_resync = Instant::now();
            if let (Some(conn), Some(p)) = (&self.conn, self.current()) {
                mpris::refresh_position(conn, &self.tx, &p.bus);
            }
        }
        if self.viz_fallback.is_some() {
            let bars = self.cfg.visualizer.bars as usize;
            self.spectrum = if self.playing() && self.cfg.visualizer.enabled {
                visualizer::idle_frame(bars, self.started)
            } else {
                vec![0.0; bars]
            };
        }
    }

    fn restart_visualizer(&mut self) {
        self.viz_restart_at = None;
        if let Some(task) = self.viz_task.take() {
            task.abort(); // drops the child process, which kills cava
        }
        if self.cfg.visualizer.enabled {
            self.viz_task = Some(visualizer::spawn(&self.cfg.visualizer, self.tx.clone()));
        } else {
            self.viz_fallback = Some("visualizer disabled".into());
        }
    }

    pub fn handle(&mut self, ev: Event) {
        match ev {
            Event::Term(TermEvent::Key(k)) if k.kind != KeyEventKind::Release => self.on_key(k),
            Event::Term(_) => {}
            Event::Player(u) => self.on_player(u),
            Event::Art { url, img, accent } => {
                if self.art_url.as_deref() == Some(url.as_str()) {
                    self.art_accent = accent;
                    self.art = img.map(|i| {
                        ThreadProtocol::new(self.resize_tx.clone(), Some(self.picker.new_resize_protocol(i)))
                    });
                }
            }
            Event::Resized(r) => {
                if let Some(art) = &mut self.art {
                    art.update_resized_protocol(r);
                }
            }
            Event::Spectrum(s) => {
                // Frames from a cava that is being replaced may still be queued; ignore them.
                if self.cfg.visualizer.enabled && self.viz_restart_at.is_none() {
                    self.viz_fallback = None;
                    self.spectrum = s;
                }
            }
            Event::VisualizerDown(reason) => {
                if self.cfg.visualizer.enabled {
                    self.viz_fallback = Some(reason);
                }
                self.on_tick();
            }
            Event::Status(s) => self.set_status(s),
        }
    }

    fn on_player(&mut self, u: Update) {
        match u {
            Update::Changed(p) => match self.players.iter_mut().find(|q| q.bus == p.bus) {
                Some(slot) => *slot = p,
                None => self.players.push(p),
            },
            Update::Removed(bus) => {
                self.players.retain(|p| p.bus != bus);
                if self.selected.as_deref() == Some(bus.as_str()) {
                    self.pinned = false;
                }
            }
            Update::Position(bus, pos) => {
                if let Some(p) = self.players.iter_mut().find(|p| p.bus == bus) {
                    p.position = pos;
                    p.position_at = Instant::now();
                }
            }
        }
        self.auto_select();
        self.sync_art();
    }

    /// Follows whatever is playing unless the user pinned a player.
    fn auto_select(&mut self) {
        let keep = match self.current() {
            Some(p) => self.pinned || p.status == Status::Playing,
            None => false,
        };
        if keep {
            return;
        }
        let pref = self.cfg.preferred_player.as_deref().map(str::to_lowercase);
        let is_pref = |p: &Player| {
            pref.as_ref()
                .is_some_and(|s| p.bus.to_lowercase().contains(s) || p.identity.to_lowercase().contains(s))
        };
        // Rank: playing, then preferred, then paused, then anything with media.
        self.selected = self
            .players
            .iter()
            .min_by_key(|p| (p.status, !is_pref(p), !p.has_media()))
            .map(|p| p.bus.clone());
    }

    fn sync_art(&mut self) {
        let url = self.current().and_then(|p| p.art_url.clone()).filter(|_| self.cfg.artwork.show);
        if url == self.art_url {
            return;
        }
        self.art = None;
        self.art_accent = None;
        self.art_url.clone_from(&url);
        if let Some(url) = url {
            media::load_art(url, self.tx.clone());
        }
    }

    fn select(&mut self, bus: String) {
        self.selected = Some(bus);
        self.pinned = true;
        self.sync_art();
    }

    fn on_key(&mut self, k: KeyEvent) {
        if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit = true;
            return;
        }
        // Overlays capture navigation keys first.
        match &mut self.overlay {
            Overlay::Selector(i) => {
                let (i, n) = (*i, self.players.len());
                match k.code {
                    KeyCode::Up | KeyCode::Char('k') if n > 0 => {
                        self.overlay = Overlay::Selector((i + n - 1) % n);
                    }
                    KeyCode::Down | KeyCode::Char('j') if n > 0 => {
                        self.overlay = Overlay::Selector((i + 1) % n);
                    }
                    KeyCode::Enter => {
                        if let Some(p) = self.players.get(i) {
                            self.select(p.bus.clone());
                        }
                        self.overlay = Overlay::None;
                    }
                    KeyCode::Esc | KeyCode::Tab | KeyCode::Char('q') => self.overlay = Overlay::None,
                    _ => {}
                }
                return;
            }
            Overlay::Help => {
                self.overlay = Overlay::None;
                return;
            }
            Overlay::Settings { .. } => return self.settings_key(k),
            Overlay::None => {}
        }
        let Some(action) = self.cfg.action_for(k) else { return };
        let step = Duration::from_secs(self.cfg.seek_step).as_micros() as i64;
        match action {
            KeyAction::Quit => self.quit = true,
            KeyAction::Help => self.overlay = Overlay::Help,
            KeyAction::Settings => self.overlay = Overlay::Settings { sel: 0, edit: None },
            KeyAction::PlayerSelector => {
                let i = self.players.iter().position(|p| Some(&p.bus) == self.selected.as_ref()).unwrap_or(0);
                self.overlay = Overlay::Selector(i);
            }
            KeyAction::NextPlayer => {
                if !self.players.is_empty() {
                    let i = self.players.iter().position(|p| Some(&p.bus) == self.selected.as_ref());
                    let next = i.map_or(0, |i| (i + 1) % self.players.len());
                    self.select(self.players[next].bus.clone());
                }
            }
            KeyAction::VisualizerMode => {
                self.cfg.visualizer.mode = self.cfg.visualizer.mode.next();
                self.dirty = true;
                self.set_status(format!("visualizer: {:?}", self.cfg.visualizer.mode).to_lowercase());
            }
            KeyAction::PlayPause => self.control(Action::PlayPause),
            KeyAction::Next => self.control(Action::Next),
            KeyAction::Previous => self.control(Action::Previous),
            KeyAction::Stop => self.control(Action::Stop),
            KeyAction::SeekForward => self.control(Action::Seek(step)),
            KeyAction::SeekBackward => self.control(Action::Seek(-step)),
        }
    }

    fn settings_key(&mut self, k: KeyEvent) {
        let Overlay::Settings { sel, edit } = &mut self.overlay else { return };
        let (mut sel, mut edit) = (*sel, edit.take());
        let fields: Vec<Field> = settings::all().collect();
        let field = fields[sel];
        let mut close = false;

        if let Some(mut buf) = edit.take() {
            // Typing into a text field.
            match k.code {
                KeyCode::Enter => match settings::set_text(&mut self.cfg, field, &buf) {
                    Ok(()) => self.dirty = true,
                    Err(e) => self.set_status(e),
                },
                KeyCode::Esc => {}
                KeyCode::Backspace => {
                    buf.pop();
                    edit = Some(buf);
                }
                KeyCode::Char(c) => {
                    buf.push(c);
                    edit = Some(buf);
                }
                _ => edit = Some(buf),
            }
        } else {
            let n = fields.len();
            let text = settings::is_text(field);
            match k.code {
                KeyCode::Up | KeyCode::Char('k') => sel = (sel + n - 1) % n,
                KeyCode::Down | KeyCode::Char('j') => sel = (sel + 1) % n,
                KeyCode::Enter if text => {
                    let cur = settings::value(&self.cfg, field);
                    edit = Some(if cur == "terminal" { String::new() } else { cur });
                }
                KeyCode::Left | KeyCode::Right | KeyCode::Char('h' | 'l' | ' ') | KeyCode::Enter if !text => {
                    let dir = if matches!(k.code, KeyCode::Left | KeyCode::Char('h')) { -1 } else { 1 };
                    match settings::adjust(&mut self.cfg, field, dir) {
                        Effect::None => {}
                        Effect::RestartVisualizer => self.viz_restart_at = Some(Instant::now()),
                        Effect::NextLaunch => self.set_status("applies next time UtaTUI starts"),
                    }
                    self.dirty = true;
                    if field == Field::ArtShow {
                        self.sync_art();
                    }
                }
                KeyCode::Char('s') => self.save(),
                KeyCode::Char('r') => self.revert(),
                KeyCode::Esc | KeyCode::Char('q') => close = true,
                _ => close = self.cfg.action_for(k) == Some(KeyAction::Settings),
            }
        }
        self.overlay = if close { Overlay::None } else { Overlay::Settings { sel, edit } };
    }

    /// Reloads settings from disk, discarding unsaved changes.
    fn revert(&mut self) {
        match Config::load(self.config_path.clone()) {
            Ok(cfg) => {
                self.cfg = cfg;
                self.dirty = false;
                self.restart_visualizer();
                self.sync_art();
                self.set_status("reverted to saved settings");
            }
            Err(e) => self.set_status(e),
        }
    }

    fn save(&mut self) {
        let Some(path) = self.config_path.clone().or_else(Config::path) else {
            return self.set_status("no config path: set $HOME or $XDG_CONFIG_HOME");
        };
        match self.cfg.save(&path) {
            Ok(()) => {
                self.dirty = false;
                self.set_status(format!("saved {}", path.display()));
            }
            Err(e) => self.set_status(format!("save failed: {e}")),
        }
    }

    fn control(&mut self, action: Action) {
        let Some(p) = self.current() else { return };
        let c = p.caps;
        let allowed = c.control
            && match action {
                Action::PlayPause => c.play || c.pause,
                Action::Next => c.next,
                Action::Previous => c.prev,
                Action::Stop => true,
                Action::Seek(_) => c.seek,
            };
        if !allowed {
            let msg = format!("{} does not support {action:?}", p.identity);
            return self.set_status(msg);
        }
        let bus = p.bus.clone();
        if let Some(conn) = &self.conn {
            mpris::send(conn, &self.tx, &bus, action);
        }
        // Players don't always signal Seeked; reflect the seek locally right away
        // (only when the player reports progress at all, so nothing is invented).
        if let Action::Seek(us) = action
            && let Some(p) = self.players.iter_mut().find(|p| p.bus == bus && p.length.is_some())
        {
            let now = p.position_now().as_micros() as i64;
            p.position = Duration::from_micros((now + us).max(0) as u64);
            p.position_at = Instant::now();
        }
    }

    pub fn mode(&self) -> Mode {
        self.cfg.visualizer.mode
    }
}
