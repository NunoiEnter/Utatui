//! TOML configuration: theme, artwork, visualizer and key bindings.
//!
//! Read from `$XDG_CONFIG_HOME/utatui/config.toml` (or `~/.config/utatui/config.toml`).
//! Every field has a default, so a partial file — or none — is fine.

use std::path::PathBuf;
use std::str::FromStr;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::Color;
use serde::{Deserialize, Serialize};

pub mod settings;

#[derive(Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Substring of a player's bus name or identity (e.g. "spotify") to prefer when several play.
    pub preferred_player: Option<String>,
    /// Seek step in seconds.
    pub seek_step: u64,
    /// Use plain ASCII glyphs instead of Unicode symbols.
    pub ascii: bool,
    /// Switch to whichever player starts playing, unless one was picked by hand.
    pub follow_playing: bool,
    pub theme: Theme,
    pub layout: LayoutCfg,
    pub artwork: Artwork,
    pub visualizer: Visualizer,
    pub keys: Keys,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Theme {
    pub accent: String,
    pub foreground: String,
    pub dim: String,
    /// Empty keeps the terminal's own background.
    pub background: String,
    /// Derive the accent color from the current artwork when there is one.
    pub accent_from_artwork: bool,
}

/// What the player screen shows besides artwork and visualizer.
#[derive(Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct LayoutCfg {
    pub show_footer: bool,
    pub show_album: bool,
    pub show_controls: bool,
    /// Scroll titles that are too long to fit instead of cutting them.
    pub scroll_title: bool,
    /// Show time remaining (-1:23) instead of the track length.
    pub remaining_time: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ArtPosition {
    /// Above the text when there is room, beside it otherwise.
    #[default]
    Auto,
    Top,
    Side,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Artwork {
    pub show: bool,
    pub position: ArtPosition,
    /// Fraction of the terminal height the artwork may occupy (0.1 – 0.9).
    pub size: f32,
    /// Graphics protocol: "auto" (query the terminal), "kitty", "sixel", "iterm2" or
    /// "halfblocks" (Unicode blocks, works everywhere, skips the terminal query).
    pub protocol: String,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Bars,
    Mirror,
    Wave,
    Blocks,
    Minimal,
}

impl Mode {
    pub const ALL: [Mode; 5] = [Mode::Bars, Mode::Mirror, Mode::Wave, Mode::Blocks, Mode::Minimal];

    pub fn next(self) -> Mode {
        let i = Self::ALL.iter().position(|&m| m == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Visualizer {
    pub enabled: bool,
    pub mode: Mode,
    pub bars: u16,
    /// Empty columns between bars.
    pub spacing: u16,
    /// Columns per bar; 0 sizes bars to fill the width.
    pub bar_width: u16,
    /// Draw a dim baseline under silent bars.
    pub baseline: bool,
    /// Passed to cava (`sensitivity`, percent).
    pub sensitivity: u32,
    /// cava frame rate; also the redraw rate while audio is flowing.
    pub framerate: u32,
    /// cava `noise_reduction` (0 – 100): higher is smoother and slower.
    pub noise_reduction: u32,
    /// Rows used by the visualizer.
    pub height: u16,
    /// Gradient from bottom to top. One color gives a solid fill; "accent" follows the accent.
    pub colors: Vec<String>,
    pub cava_path: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Keys {
    pub quit: Vec<String>,
    pub play_pause: Vec<String>,
    pub next: Vec<String>,
    pub previous: Vec<String>,
    pub stop: Vec<String>,
    pub seek_forward: Vec<String>,
    pub seek_backward: Vec<String>,
    pub player_selector: Vec<String>,
    pub next_player: Vec<String>,
    pub visualizer_mode: Vec<String>,
    pub help: Vec<String>,
    pub settings: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAction {
    Quit,
    PlayPause,
    Next,
    Previous,
    Stop,
    SeekForward,
    SeekBackward,
    PlayerSelector,
    NextPlayer,
    VisualizerMode,
    Help,
    Settings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            preferred_player: None,
            seek_step: 5,
            ascii: false,
            follow_playing: true,
            theme: Theme::default(),
            layout: LayoutCfg::default(),
            artwork: Artwork::default(),
            visualizer: Visualizer::default(),
            keys: Keys::default(),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: "#b4a7f5".into(),
            foreground: "#e6e6ef".into(),
            dim: "#6e6c86".into(),
            background: String::new(),
            accent_from_artwork: true,
        }
    }
}

impl Default for LayoutCfg {
    fn default() -> Self {
        Self {
            show_footer: true,
            show_album: true,
            show_controls: true,
            scroll_title: true,
            remaining_time: false,
        }
    }
}

impl Default for Artwork {
    fn default() -> Self {
        Self { show: true, position: ArtPosition::Auto, size: 0.5, protocol: "auto".into() }
    }
}

impl Default for Visualizer {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: Mode::Bars,
            bars: 48,
            spacing: 1,
            bar_width: 0,
            baseline: true,
            sensitivity: 100,
            framerate: 60,
            noise_reduction: 77,
            height: 8,
            colors: vec!["#5e60ce".into(), "#7b6cf6".into(), "#b4a7f5".into(), "#f2c4de".into()],
            cava_path: "cava".into(),
        }
    }
}

fn keys(list: &[&str]) -> Vec<String> {
    list.iter().map(std::string::ToString::to_string).collect()
}

impl Default for Keys {
    fn default() -> Self {
        Self {
            quit: keys(&["q", "esc"]),
            play_pause: keys(&["space"]),
            next: keys(&["n"]),
            previous: keys(&["p"]),
            stop: keys(&["s"]),
            seek_forward: keys(&["right", "l"]),
            seek_backward: keys(&["left", "h"]),
            player_selector: keys(&["tab"]),
            next_player: keys(&["]"]),
            visualizer_mode: keys(&["v"]),
            help: keys(&["?"]),
            settings: keys(&["o", ","]),
        }
    }
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("utatui").join("config.toml"))
    }

    /// Loads the config file; a missing file yields defaults, a malformed one is an error.
    pub fn load(path: Option<PathBuf>) -> Result<Self, String> {
        let Some(path) = path.or_else(Self::path) else { return Ok(Self::default()) };
        match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    /// Writes the config as TOML, creating the directory if needed.
    pub fn save(&self, path: &std::path::Path) -> Result<(), String> {
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn action_for(&self, key: KeyEvent) -> Option<KeyAction> {
        let k = &self.keys;
        let table = [
            (&k.quit, KeyAction::Quit),
            (&k.play_pause, KeyAction::PlayPause),
            (&k.next, KeyAction::Next),
            (&k.previous, KeyAction::Previous),
            (&k.stop, KeyAction::Stop),
            (&k.seek_forward, KeyAction::SeekForward),
            (&k.seek_backward, KeyAction::SeekBackward),
            (&k.player_selector, KeyAction::PlayerSelector),
            (&k.next_player, KeyAction::NextPlayer),
            (&k.visualizer_mode, KeyAction::VisualizerMode),
            (&k.help, KeyAction::Help),
            (&k.settings, KeyAction::Settings),
        ];
        table.into_iter().find(|(names, _)| names.iter().any(|n| key_matches(n, key))).map(|(_, a)| a)
    }
}

/// Matches a binding like `"space"`, `"n"`, `"N"` or `"ctrl+n"` against a key event.
fn key_matches(binding: &str, key: KeyEvent) -> bool {
    let (ctrl, name) = match binding.to_lowercase().strip_prefix("ctrl+") {
        Some(rest) => (true, rest.to_string()),
        None => (false, binding.to_string()),
    };
    if ctrl != key.modifiers.contains(KeyModifiers::CONTROL) {
        return false;
    }
    let code = match name.to_lowercase().as_str() {
        "space" => KeyCode::Char(' '),
        "tab" => KeyCode::Tab,
        "enter" => KeyCode::Enter,
        "esc" => KeyCode::Esc,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        _ => match name.chars().collect::<Vec<_>>()[..] {
            [c] => KeyCode::Char(if ctrl { c.to_ascii_lowercase() } else { c }),
            _ => return false,
        },
    };
    key.code == code
}

/// Parses `#rrggbb` or a color name; falls back to the terminal default on garbage.
pub fn color(s: &str) -> Color {
    Color::from_str(s).unwrap_or(Color::Reset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_parse_and_bind() {
        let cfg: Config = toml::from_str("[visualizer]\nmode = \"mirror\"\nbars = 10").unwrap();
        assert_eq!(cfg.visualizer.mode, Mode::Mirror);
        assert_eq!(cfg.visualizer.bars, 10);
        let space = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        assert_eq!(cfg.action_for(space), Some(KeyAction::PlayPause));
        let shift_n = KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT);
        assert_eq!(cfg.action_for(shift_n), None);
        assert!(key_matches("ctrl+n", KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL)));
        assert!(toml::from_str::<Config>("bogus = 1").is_err());
        toml::from_str::<Config>(include_str!("../../config.example.toml")).unwrap();
        // Saving must round-trip.
        let saved = toml::to_string_pretty(&cfg).unwrap();
        assert_eq!(toml::from_str::<Config>(&saved).unwrap().visualizer.bars, 10);
    }
}
