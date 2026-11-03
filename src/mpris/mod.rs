//! MPRIS player discovery, state tracking and playback control over D-Bus.
//!
//! Everything is signal driven: `NameOwnerChanged` tells us when players appear or vanish,
//! `PropertiesChanged` and `Seeked` tell us when their state changes. No polling.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use futures_util::stream::select_all;
use tokio::sync::mpsc::UnboundedSender;
use zbus::{Connection, MessageStream, OwnedVariant, Variant};

use crate::app::Event;

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";
const IFACE_ROOT: &str = "org.mpris.MediaPlayer2";
const IFACE_PLAYER: &str = "org.mpris.MediaPlayer2.Player";
const PROPS: &str = "org.freedesktop.DBus.Properties";
/// playerctld re-exports whichever player is active; listing it would duplicate a real player.
const IGNORED: &[&str] = &["org.mpris.MediaPlayer2.playerctld"];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Playing,
    Paused,
    #[default]
    Stopped,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Caps {
    pub control: bool,
    pub play: bool,
    pub pause: bool,
    pub next: bool,
    pub prev: bool,
    pub seek: bool,
}

#[derive(Clone, Debug)]
pub struct Player {
    /// Well-known bus name, e.g. `org.mpris.MediaPlayer2.firefox.instance_1_89`.
    pub bus: String,
    /// Human-readable name from the `Identity` property.
    pub identity: String,
    pub status: Status,
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    pub art_url: Option<String>,
    /// Page the media plays on (`xesam:url`), e.g. a YouTube watch URL.
    pub page_url: String,
    pub length: Option<Duration>,
    /// Position reported by the player at `position_at`; interpolated locally while playing.
    pub position: Duration,
    pub position_at: Instant,
    pub rate: f64,
    pub caps: Caps,
}

impl Player {
    /// Current position, extrapolated from the last report so the progress bar moves smoothly.
    pub fn position_now(&self) -> Duration {
        let mut pos = self.position;
        if self.status == Status::Playing {
            pos += self.position_at.elapsed().mul_f64(self.rate.max(0.0));
        }
        match self.length {
            Some(len) => pos.min(len),
            None => pos,
        }
    }

    pub fn has_media(&self) -> bool {
        !self.title.is_empty() || self.status != Status::Stopped
    }
}

#[derive(Debug)]
pub enum Update {
    Changed(Player),
    Removed(String),
    Position(String, Duration),
}

/// Discovers players and streams their state to the app until the bus connection dies.
pub async fn watch(conn: Connection, tx: UnboundedSender<Event>) -> zbus::Result<()> {
    // Subscribe before listing names so nothing that appears in between is missed.
    let rules = [
        "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0namespace='org.mpris.MediaPlayer2'",
        "type='signal',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',path='/org/mpris/MediaPlayer2'",
        "type='signal',interface='org.mpris.MediaPlayer2.Player',member='Seeked',path='/org/mpris/MediaPlayer2'",
    ];
    let mut streams = Vec::new();
    for rule in rules {
        streams.push(MessageStream::for_match_rule(rule, &conn, None).await?);
    }
    let mut signals = select_all(streams);

    // Signals carry the sender's unique name (`:1.89`); map it back to the well-known name.
    let mut owners: HashMap<String, String> = HashMap::new();
    let names: Vec<String> = dbus_call(&conn, "ListNames", &()).await?.body().deserialize()?;
    for name in names.into_iter().filter(|n| is_player(n)) {
        if let Ok(msg) = dbus_call(&conn, "GetNameOwner", &(name.as_str(),)).await
            && let Ok(owner) = msg.body().deserialize::<String>()
        {
            owners.insert(owner, name.clone());
            spawn_refresh(&conn, &tx, name);
        }
    }

    while let Some(msg) = signals.next().await {
        let Ok(msg) = msg else { continue };
        let header = msg.header();
        let member = header.member().map(std::string::ToString::to_string).unwrap_or_default();
        let sender = header.sender().map(std::string::ToString::to_string).unwrap_or_default();
        match member.as_str() {
            "NameOwnerChanged" => {
                let Ok((name, old, new)) = msg.body().deserialize::<(String, String, String)>() else {
                    continue;
                };
                if !is_player(&name) {
                    continue;
                }
                owners.remove(&old);
                if new.is_empty() {
                    let _ = tx.send(Event::Player(Update::Removed(name)));
                } else {
                    owners.insert(new, name.clone());
                    spawn_refresh(&conn, &tx, name);
                }
            }
            "PropertiesChanged" => {
                // Re-reading everything is simpler than merging partial updates, and also
                // picks up Position, which players are not required to signal.
                if let Some(name) = owners.get(&sender) {
                    spawn_refresh(&conn, &tx, name.clone());
                }
            }
            "Seeked" => {
                if let (Some(name), Ok(us)) = (owners.get(&sender), msg.body().deserialize::<i64>()) {
                    let _ = tx.send(Event::Player(Update::Position(name.clone(), micros(us))));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn is_player(name: &str) -> bool {
    name.starts_with(PREFIX) && !IGNORED.contains(&name)
}

fn host(url: &str) -> String {
    let rest = url.split_once("://").map_or("", |(_, r)| r);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    host.trim_start_matches("www.").to_string()
}

fn micros(us: i64) -> Duration {
    Duration::from_micros(us.max(0) as u64)
}

async fn dbus_call<B>(conn: &Connection, method: &str, body: &B) -> zbus::Result<zbus::Message>
where
    B: serde::Serialize + zbus::wire::DynamicType,
{
    conn.call_method(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus"),
        method,
        body,
    )
    .await
}

fn spawn_refresh(conn: &Connection, tx: &UnboundedSender<Event>, bus: String) {
    let (conn, tx) = (conn.clone(), tx.clone());
    tokio::spawn(async move {
        // A hung player must not leave a task waiting forever.
        if let Ok(Ok(player)) = tokio::time::timeout(Duration::from_secs(3), fetch(&conn, &bus)).await {
            let _ = tx.send(Event::Player(Update::Changed(player)));
        }
    });
}

/// Re-reads `Position`. Per the MPRIS spec players don't signal position changes, so
/// clients are expected to query it; this corrects drift from local interpolation.
pub fn refresh_position(conn: &Connection, tx: &UnboundedSender<Event>, bus: &str) {
    let (conn, tx, bus) = (conn.clone(), tx.clone(), bus.to_string());
    tokio::spawn(async move {
        let call =
            conn.call_method(Some(bus.as_str()), PATH, Some(PROPS), "Get", &(IFACE_PLAYER, "Position"));
        if let Ok(Ok(msg)) = tokio::time::timeout(Duration::from_secs(2), call).await
            && let Some(us) = msg.body().deserialize::<OwnedVariant>().ok().and_then(|v| as_i64(&v))
        {
            let _ = tx.send(Event::Player(Update::Position(bus, micros(us))));
        }
    });
}

async fn get_all(conn: &Connection, bus: &str, iface: &str) -> zbus::Result<HashMap<String, OwnedVariant>> {
    let msg = conn.call_method(Some(bus), PATH, Some(PROPS), "GetAll", &(iface,)).await?;
    msg.body().deserialize()
}

async fn fetch(conn: &Connection, bus: &str) -> zbus::Result<Player> {
    let p = get_all(conn, bus, IFACE_PLAYER).await?;
    let root = get_all(conn, bus, IFACE_ROOT).await.unwrap_or_default();

    let flag = |k: &str| p.get(k).and_then(|v| as_bool(v)).unwrap_or(false);
    let status = match p.get("PlaybackStatus").and_then(|v| as_string(v)).as_deref() {
        Some("Playing") => Status::Playing,
        Some("Paused") => Status::Paused,
        _ => Status::Stopped,
    };
    let empty = HashMap::new();
    let meta: HashMap<String, &Variant> = match p.get("Metadata").map(|v| unwrap(v)) {
        Some(Variant::Dict(d)) => d.iter().filter_map(|(k, v)| Some((as_string(k)?, unwrap(v)))).collect(),
        _ => empty,
    };
    let m_str = |k: &str| meta.get(k).and_then(|v| as_string(v)).unwrap_or_default();

    let identity = root
        .get("Identity")
        .and_then(|v| as_string(v))
        .unwrap_or_else(|| bus.trim_start_matches(PREFIX).split('.').next().unwrap_or(bus).to_string());

    Ok(Player {
        bus: bus.to_string(),
        identity,
        status,
        title: m_str("xesam:title"),
        artists: meta.get("xesam:artist").map(|v| as_strings(v)).unwrap_or_default(),
        // Browsers leave album empty; the page's host ("youtube.com") is the next best source.
        album: Some(m_str("xesam:album"))
            .filter(|a| !a.is_empty())
            .unwrap_or_else(|| host(&m_str("xesam:url"))),
        page_url: m_str("xesam:url"),
        art_url: meta.get("mpris:artUrl").and_then(|v| as_string(v)).filter(|s| !s.is_empty()),
        length: meta.get("mpris:length").and_then(|v| as_i64(v)).filter(|&l| l > 0).map(micros),
        position: p.get("Position").and_then(|v| as_i64(v)).map(micros).unwrap_or_default(),
        position_at: Instant::now(),
        rate: p.get("Rate").and_then(|v| as_f64(v)).unwrap_or(1.0),
        caps: Caps {
            control: flag("CanControl"),
            play: flag("CanPlay"),
            pause: flag("CanPause"),
            next: flag("CanGoNext"),
            prev: flag("CanGoPrevious"),
            seek: flag("CanSeek"),
        },
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn host() {
        assert_eq!(super::host("https://www.youtube.com/watch?v=x"), "youtube.com");
        assert_eq!(super::host("file:///home/a.mp3"), "");
        assert_eq!(super::host(""), "");
    }
}

// --- Variant helpers: players disagree on integer widths, so accept any. ---

fn unwrap<'a>(v: &'a Variant<'a>) -> &'a Variant<'a> {
    match v {
        Variant::Variant(inner) => unwrap(inner),
        other => other,
    }
}

fn as_string(v: &Variant) -> Option<String> {
    match unwrap(v) {
        Variant::Str(s) => Some(s.to_string()),
        Variant::ObjectPath(p) => Some(p.to_string()),
        _ => None,
    }
}

fn as_strings(v: &Variant) -> Vec<String> {
    match unwrap(v) {
        Variant::Array(a) => a.iter().filter_map(as_string).collect(),
        other => as_string(other).into_iter().collect(),
    }
}

fn as_i64(v: &Variant) -> Option<i64> {
    Some(match *unwrap(v) {
        Variant::I64(x) => x,
        Variant::U64(x) => x as i64,
        Variant::I32(x) => x as i64,
        Variant::U32(x) => x as i64,
        Variant::F64(x) => x as i64,
        _ => return None,
    })
}

fn as_f64(v: &Variant) -> Option<f64> {
    match *unwrap(v) {
        Variant::F64(x) => Some(x),
        _ => as_i64(v).map(|x| x as f64),
    }
}

fn as_bool(v: &Variant) -> Option<bool> {
    match *unwrap(v) {
        Variant::Bool(b) => Some(b),
        _ => None,
    }
}

// --- Controls ---

#[derive(Clone, Copy, Debug)]
pub enum Action {
    PlayPause,
    Next,
    Previous,
    Stop,
    /// Relative seek in microseconds.
    Seek(i64),
}

/// Sends a control command without blocking the caller; failures are reported as status text.
pub fn send(conn: &Connection, tx: &UnboundedSender<Event>, bus: &str, action: Action) {
    let (conn, tx, bus) = (conn.clone(), tx.clone(), bus.to_string());
    tokio::spawn(async move {
        let res = match action {
            Action::PlayPause => player_call(&conn, &bus, "PlayPause", &()).await,
            Action::Next => player_call(&conn, &bus, "Next", &()).await,
            Action::Previous => player_call(&conn, &bus, "Previous", &()).await,
            Action::Stop => player_call(&conn, &bus, "Stop", &()).await,
            Action::Seek(us) => player_call(&conn, &bus, "Seek", &(us,)).await,
        };
        if let Err(e) = res {
            let _ = tx.send(Event::Status(format!("{action:?} failed: {e}")));
        }
    });
}

async fn player_call<B>(conn: &Connection, bus: &str, method: &str, body: &B) -> zbus::Result<()>
where
    B: serde::Serialize + zbus::wire::DynamicType,
{
    conn.call_method(Some(bus), PATH, Some(IFACE_PLAYER), method, body).await?;
    Ok(())
}
