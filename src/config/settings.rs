//! In-app settings: the editable fields, their current values, and how to change them.

use std::str::FromStr;

use ratatui::style::Color;

use super::{Config, Mode};

pub struct Preset {
    pub name: &'static str,
    pub accent: &'static str,
    pub foreground: &'static str,
    pub dim: &'static str,
    pub gradient: &'static [&'static str],
}

pub const THEMES: &[Preset] = &[
    Preset {
        name: "lavender",
        accent: "#b4a7f5",
        foreground: "#e6e6ef",
        dim: "#6e6c86",
        gradient: &["#5e60ce", "#7b6cf6", "#b4a7f5", "#f2c4de"],
    },
    Preset {
        name: "catppuccin",
        accent: "#cba6f7",
        foreground: "#cdd6f4",
        dim: "#6c7086",
        gradient: &["#89b4fa", "#b4befe", "#cba6f7", "#f5c2e7"],
    },
    Preset {
        name: "rose-pine",
        accent: "#ebbcba",
        foreground: "#e0def4",
        dim: "#6e6a86",
        gradient: &["#31748f", "#9ccfd8", "#c4a7e7", "#ebbcba"],
    },
    Preset {
        name: "tokyo-night",
        accent: "#7aa2f7",
        foreground: "#c0caf5",
        dim: "#565f89",
        gradient: &["#2ac3de", "#7aa2f7", "#bb9af7", "#f7768e"],
    },
    Preset {
        name: "nord",
        accent: "#88c0d0",
        foreground: "#eceff4",
        dim: "#616e88",
        gradient: &["#5e81ac", "#81a1c1", "#88c0d0", "#8fbcbb"],
    },
    Preset {
        name: "gruvbox",
        accent: "#fabd2f",
        foreground: "#ebdbb2",
        dim: "#7c6f64",
        gradient: &["#cc241d", "#d65d0e", "#d79921", "#fabd2f"],
    },
    Preset {
        name: "sakura",
        accent: "#f4a7c0",
        foreground: "#f6e9ee",
        dim: "#86707a",
        gradient: &["#8e5572", "#c97b9a", "#f4a7c0", "#ffe0ea"],
    },
    Preset {
        name: "mono",
        accent: "#ffffff",
        foreground: "#d0d0d0",
        dim: "#5a5a5a",
        gradient: &["#3a3a3a", "#9a9a9a", "#ffffff"],
    },
    // ANSI names follow the terminal's own color scheme.
    Preset {
        name: "terminal",
        accent: "magenta",
        foreground: "reset",
        dim: "darkgray",
        gradient: &["blue", "magenta"],
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    ThemePreset,
    AccentFromArt,
    Accent,
    Foreground,
    Dim,
    Background,
    ShowFooter,
    ShowAlbum,
    ShowControls,
    ScrollTitle,
    RemainingTime,
    ArtShow,
    ArtPosition,
    ArtShape,
    ArtHd,
    ArtSize,
    ArtProtocol,
    VizEnabled,
    VizMode,
    VizGradient,
    VizColors,
    VizHeight,
    VizBars,
    VizBarWidth,
    VizSpacing,
    VizBaseline,
    VizSensitivity,
    VizFramerate,
    VizSmoothing,
    FollowPlaying,
    PreferredPlayer,
    SeekStep,
    Ascii,
}

use Field::{
    Accent, AccentFromArt, ArtHd, ArtPosition, ArtProtocol, ArtShape, ArtShow, ArtSize, Ascii, Background,
    Dim, FollowPlaying, Foreground, PreferredPlayer, RemainingTime, ScrollTitle, SeekStep, ShowAlbum,
    ShowControls, ShowFooter, ThemePreset, VizBarWidth, VizBars, VizBaseline, VizColors, VizEnabled,
    VizFramerate, VizGradient, VizHeight, VizMode, VizSensitivity, VizSmoothing, VizSpacing,
};

/// Settings tabs, in display order.
pub const TABS: &[(&str, &[Field])] = &[
    ("Theme", &[ThemePreset, AccentFromArt, Accent, Foreground, Dim, Background]),
    ("Layout", &[ShowFooter, ShowAlbum, ShowControls, ScrollTitle, RemainingTime]),
    ("Artwork", &[ArtShow, ArtSize, ArtShape, ArtHd, ArtPosition, ArtProtocol]),
    (
        "Visualizer",
        &[
            VizEnabled,
            VizMode,
            VizGradient,
            VizColors,
            VizHeight,
            VizBars,
            VizBarWidth,
            VizSpacing,
            VizBaseline,
            VizSensitivity,
            VizFramerate,
            VizSmoothing,
        ],
    ),
    ("Player", &[FollowPlaying, PreferredPlayer, SeekStep]),
    ("General", &[Ascii]),
];

/// What a change needs beyond updating the config.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    /// cava reads these settings at startup.
    RestartVisualizer,
    /// Only takes effect the next time UtaTUI starts.
    NextLaunch,
}

const PROTOCOLS: &[&str] = &["auto", "kitty", "sixel", "iterm2", "halfblocks"];
const FRAMERATES: &[u32] = &[30, 60, 90, 120, 144];
const SHAPES: [super::ArtShape; 2] = [super::ArtShape::Original, super::ArtShape::Square];
const POSITIONS: [super::ArtPosition; 3] =
    [super::ArtPosition::Auto, super::ArtPosition::Top, super::ArtPosition::Side];

pub fn label(f: Field) -> &'static str {
    match f {
        ThemePreset => "Preset",
        AccentFromArt => "Accent from artwork",
        Accent => "Accent",
        Foreground => "Text",
        Dim => "Muted text",
        Background => "Background",
        ShowFooter => "Status bar",
        ShowAlbum => "Album line",
        ShowControls => "Controls",
        ScrollTitle => "Scroll long titles",
        RemainingTime => "Time remaining",
        ArtShow => "Show artwork",
        ArtPosition => "Position",
        ArtShape => "Shape",
        ArtHd => "HD thumbnails",
        ArtSize => "Size",
        ArtProtocol => "Protocol",
        VizEnabled => "Enabled",
        VizMode => "Style",
        VizGradient => "Gradient",
        VizColors => "Colors",
        VizHeight => "Height",
        VizBars => "Bars",
        VizBarWidth => "Bar width",
        VizSpacing => "Spacing",
        VizBaseline => "Baseline",
        VizSensitivity => "Sensitivity",
        VizFramerate => "Frame rate",
        VizSmoothing => "Smoothing",
        FollowPlaying => "Follow playing",
        PreferredPlayer => "Preferred player",
        SeekStep => "Seek step",
        Ascii => "ASCII glyphs",
    }
}

/// One-line explanation shown under the settings list.
pub fn help(f: Field) -> &'static str {
    match f {
        ThemePreset => "Color theme. Sets accent, text and visualizer colors together.",
        AccentFromArt => "Pick the accent color from the current artwork.",
        Accent => "Highlight color, e.g. #cba6f7 or magenta. Press enter to type.",
        Foreground => "Main text color. Press enter to type.",
        Dim => "Color for secondary text such as times and hints.",
        Background => "Background color. Leave empty to keep the terminal's own.",
        ShowFooter => "Bottom bar with the player name and status.",
        ShowAlbum => "Line with the album, or the website for browser media.",
        ShowControls => "Previous / play / next buttons.",
        ScrollTitle => "Scroll titles that are too long to fit instead of cutting them.",
        RemainingTime => "Show time left (-1:23) instead of the track length.",
        ArtShow => "Show the album art or video thumbnail.",
        ArtPosition => "auto: above when there's room, beside otherwise. top / side: always.",
        ArtSize => "How much of the screen height the artwork may use.",
        ArtShape => "original keeps the image's shape; square crops it like an album cover.",
        ArtHd => "Download sharp YouTube thumbnails instead of the browser's small preview.",
        ArtProtocol => "Image method. Use halfblocks if images look wrong. Needs restart.",
        VizEnabled => "Show the audio visualizer (needs cava installed).",
        VizMode => "bars, mirror, wave, blocks or minimal. Also cycle with v.",
        VizGradient => "Visualizer color scheme. \"accent\" follows the accent color.",
        VizColors => "Gradient colors bottom to top, comma separated. Press enter to type.",
        VizHeight => "Rows the visualizer uses when there is room.",
        VizBars => "Number of frequency bands cava measures.",
        VizBarWidth => "Columns per bar. auto fills the available width.",
        VizSpacing => "Empty columns between bars.",
        VizBaseline => "Dim line under silent bars, so silence doesn't look broken.",
        VizSensitivity => "How tall bars get for a given loudness.",
        VizFramerate => "Visualizer updates per second. Lower uses less CPU.",
        VizSmoothing => "Higher is smoother and calmer, lower is snappier.",
        FollowPlaying => "Switch to whichever player starts playing.",
        PreferredPlayer => "Prefer this player when several play, e.g. spotify. Enter to type.",
        SeekStep => "Seconds to jump with the seek keys.",
        Ascii => "Plain ASCII symbols for fonts without good Unicode support.",
    }
}

/// Fields edited by typing rather than stepping.
pub fn is_text(f: Field) -> bool {
    matches!(f, Accent | Foreground | Dim | Background | VizColors | PreferredPlayer)
}

fn on_off(b: bool) -> String {
    if b { "on" } else { "off" }.into()
}

pub fn value(c: &Config, f: Field) -> String {
    let (t, l, a, v) = (&c.theme, &c.layout, &c.artwork, &c.visualizer);
    match f {
        ThemePreset => THEMES
            .iter()
            .find(|p| p.accent == t.accent && p.foreground == t.foreground && p.dim == t.dim)
            .map_or("custom", |p| p.name)
            .into(),
        AccentFromArt => on_off(t.accent_from_artwork),
        Accent => t.accent.clone(),
        Foreground => t.foreground.clone(),
        Dim => t.dim.clone(),
        Background => {
            if t.background.is_empty() {
                "terminal".into()
            } else {
                t.background.clone()
            }
        }
        ShowFooter => on_off(l.show_footer),
        ShowAlbum => on_off(l.show_album),
        ShowControls => on_off(l.show_controls),
        ScrollTitle => on_off(l.scroll_title),
        RemainingTime => on_off(l.remaining_time),
        ArtShow => on_off(a.show),
        ArtPosition => format!("{:?}", a.position).to_lowercase(),
        ArtShape => format!("{:?}", a.shape).to_lowercase(),
        ArtHd => on_off(a.hd_thumbnails),
        ArtSize => format!("{:.0}%", a.size * 100.0),
        ArtProtocol => a.protocol.clone(),
        VizEnabled => on_off(v.enabled),
        VizMode => format!("{:?}", v.mode).to_lowercase(),
        VizGradient => gradient_name(&v.colors).into(),
        VizColors => v.colors.join(", "),
        VizHeight => format!("{} rows", v.height),
        VizBars => v.bars.to_string(),
        VizBarWidth => {
            if v.bar_width == 0 {
                "auto".into()
            } else {
                v.bar_width.to_string()
            }
        }
        VizSpacing => v.spacing.to_string(),
        VizBaseline => on_off(v.baseline),
        VizSensitivity => format!("{}%", v.sensitivity),
        VizFramerate => format!("{} fps", v.framerate),
        VizSmoothing => v.noise_reduction.to_string(),
        FollowPlaying => on_off(c.follow_playing),
        PreferredPlayer => c.preferred_player.clone().unwrap_or_else(|| "none".into()),
        SeekStep => format!("{}s", c.seek_step),
        Ascii => on_off(c.ascii),
    }
}

fn gradient_name(colors: &[String]) -> &'static str {
    if colors == ["accent"] {
        return "accent";
    }
    THEMES.iter().find(|p| p.gradient.iter().eq(colors.iter())).map_or("custom", |p| p.name)
}

fn step<T>(x: T, dir: i32, by: T, min: T, max: T) -> T
where
    T: Copy + PartialOrd + std::ops::Add<Output = T> + std::ops::Sub<Output = T>,
{
    let next = if dir > 0 {
        x + by
    } else if x > min + by {
        x - by
    } else {
        min
    };
    if next > max {
        max
    } else if next < min {
        min
    } else {
        next
    }
}

/// Cycles `cur` through `options` in direction `dir`, wrapping around.
fn cycle<T: PartialEq + Copy>(options: &[T], cur: T, dir: i32) -> T {
    let i = options.iter().position(|&o| o == cur).unwrap_or(0) as i32;
    options[(i + dir).rem_euclid(options.len() as i32) as usize]
}

/// What a field change requires, so resetting and stepping report the same effect.
fn effect_of(f: Field) -> Effect {
    match f {
        VizEnabled | VizBars | VizSensitivity | VizFramerate | VizSmoothing => Effect::RestartVisualizer,
        ArtProtocol => Effect::NextLaunch,
        _ => Effect::None,
    }
}

/// Steps a non-text field left (`dir < 0`) or right (`dir > 0`).
pub fn adjust(c: &mut Config, f: Field, dir: i32) -> Effect {
    let (t, l, a, v) = (&mut c.theme, &mut c.layout, &mut c.artwork, &mut c.visualizer);
    let flip = |b: &mut bool| *b = !*b;
    match f {
        ThemePreset => {
            let names: Vec<&str> = THEMES.iter().map(|p| p.name).collect();
            let cur = THEMES
                .iter()
                .find(|p| p.accent == t.accent && p.foreground == t.foreground)
                .map_or("", |p| p.name);
            let start = if cur.is_empty() && dir > 0 { names[names.len() - 1] } else { cur };
            let name = cycle(&names, start, dir);
            let p = THEMES.iter().find(|p| p.name == name).expect("name comes from THEMES");
            t.accent = p.accent.into();
            t.foreground = p.foreground.into();
            t.dim = p.dim.into();
            // Keep an "accent" gradient: it already follows the new theme.
            if v.colors != ["accent"] {
                v.colors = p.gradient.iter().map(std::string::ToString::to_string).collect();
            }
        }
        AccentFromArt => flip(&mut t.accent_from_artwork),
        ShowFooter => flip(&mut l.show_footer),
        ShowAlbum => flip(&mut l.show_album),
        ShowControls => flip(&mut l.show_controls),
        ScrollTitle => flip(&mut l.scroll_title),
        RemainingTime => flip(&mut l.remaining_time),
        ArtShow => flip(&mut a.show),
        ArtPosition => a.position = cycle(&POSITIONS, a.position, dir),
        ArtShape => a.shape = cycle(&SHAPES, a.shape, dir),
        ArtHd => flip(&mut a.hd_thumbnails),
        ArtSize => a.size = (step(a.size, dir, 0.05, 0.15, 0.9) * 100.0).round() / 100.0,
        ArtProtocol => a.protocol = cycle(PROTOCOLS, a.protocol.as_str(), dir).into(),
        VizEnabled => flip(&mut v.enabled),
        VizMode => v.mode = cycle(&Mode::ALL, v.mode, dir),
        VizGradient => {
            let mut names: Vec<&str> = vec!["accent"];
            names.extend(THEMES.iter().map(|p| p.name));
            let name = cycle(&names, gradient_name(&v.colors), dir);
            v.colors = match THEMES.iter().find(|p| p.name == name) {
                Some(p) => p.gradient.iter().map(std::string::ToString::to_string).collect(),
                None => vec!["accent".into()],
            };
        }
        VizHeight => v.height = step(v.height, dir, 1, 1, 30),
        VizBars => v.bars = step(v.bars, dir, 4, 4, 256),
        VizBarWidth => v.bar_width = step(v.bar_width, dir, 1, 0, 8),
        VizSpacing => v.spacing = step(v.spacing, dir, 1, 0, 4),
        VizBaseline => flip(&mut v.baseline),
        VizSensitivity => v.sensitivity = step(v.sensitivity, dir, 10, 10, 500),
        VizFramerate => v.framerate = cycle(FRAMERATES, v.framerate, dir),
        VizSmoothing => v.noise_reduction = step(v.noise_reduction, dir, 5, 0, 100),
        FollowPlaying => flip(&mut c.follow_playing),
        SeekStep => c.seek_step = step(c.seek_step, dir, 1, 1, 60),
        Ascii => flip(&mut c.ascii),
        Accent | Foreground | Dim | Background | VizColors | PreferredPlayer => return Effect::None,
    }
    effect_of(f)
}

/// Restores a field to its default value.
pub fn reset(c: &mut Config, f: Field) -> Effect {
    let d = Config::default();
    match f {
        ThemePreset => {
            c.theme.accent = d.theme.accent;
            c.theme.foreground = d.theme.foreground;
            c.theme.dim = d.theme.dim;
            c.visualizer.colors = d.visualizer.colors;
        }
        AccentFromArt => c.theme.accent_from_artwork = d.theme.accent_from_artwork,
        Accent => c.theme.accent = d.theme.accent,
        Foreground => c.theme.foreground = d.theme.foreground,
        Dim => c.theme.dim = d.theme.dim,
        Background => c.theme.background = d.theme.background,
        ShowFooter => c.layout.show_footer = d.layout.show_footer,
        ShowAlbum => c.layout.show_album = d.layout.show_album,
        ShowControls => c.layout.show_controls = d.layout.show_controls,
        ScrollTitle => c.layout.scroll_title = d.layout.scroll_title,
        RemainingTime => c.layout.remaining_time = d.layout.remaining_time,
        ArtShow => c.artwork.show = d.artwork.show,
        ArtPosition => c.artwork.position = d.artwork.position,
        ArtShape => c.artwork.shape = d.artwork.shape,
        ArtHd => c.artwork.hd_thumbnails = d.artwork.hd_thumbnails,
        ArtSize => c.artwork.size = d.artwork.size,
        ArtProtocol => c.artwork.protocol = d.artwork.protocol,
        VizEnabled => c.visualizer.enabled = d.visualizer.enabled,
        VizMode => c.visualizer.mode = d.visualizer.mode,
        VizGradient | VizColors => c.visualizer.colors = d.visualizer.colors,
        VizHeight => c.visualizer.height = d.visualizer.height,
        VizBars => c.visualizer.bars = d.visualizer.bars,
        VizBarWidth => c.visualizer.bar_width = d.visualizer.bar_width,
        VizSpacing => c.visualizer.spacing = d.visualizer.spacing,
        VizBaseline => c.visualizer.baseline = d.visualizer.baseline,
        VizSensitivity => c.visualizer.sensitivity = d.visualizer.sensitivity,
        VizFramerate => c.visualizer.framerate = d.visualizer.framerate,
        VizSmoothing => c.visualizer.noise_reduction = d.visualizer.noise_reduction,
        FollowPlaying => c.follow_playing = d.follow_playing,
        PreferredPlayer => c.preferred_player = d.preferred_player,
        SeekStep => c.seek_step = d.seek_step,
        Ascii => c.ascii = d.ascii,
    }
    effect_of(f)
}

fn valid_color(s: &str) -> bool {
    s == "accent" || Color::from_str(s).is_ok()
}

/// Sets a text field from user input, validating colors.
pub fn set_text(c: &mut Config, f: Field, input: &str) -> Result<(), String> {
    let s = input.trim();
    let check = |s: &str| {
        if valid_color(s) && s != "accent" { Ok(s.to_string()) } else { Err(format!("not a color: `{s}`")) }
    };
    match f {
        Accent => c.theme.accent = check(s)?,
        Foreground => c.theme.foreground = check(s)?,
        Dim => c.theme.dim = check(s)?,
        Background => {
            c.theme.background = if s.is_empty() || s == "terminal" { String::new() } else { check(s)? }
        }
        VizColors => {
            let colors: Vec<String> =
                s.split([',', ' ']).filter(|x| !x.is_empty()).map(str::to_string).collect();
            if let Some(bad) = colors.iter().find(|x| !valid_color(x)) {
                return Err(format!("not a color: `{bad}`"));
            }
            if colors.is_empty() {
                return Err("enter at least one color".into());
            }
            c.visualizer.colors = colors;
        }
        PreferredPlayer => {
            c.preferred_player = (!s.is_empty() && s != "none").then(|| s.to_string());
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjusting_and_editing() {
        let mut c = Config::default();
        assert_eq!(value(&c, ThemePreset), "lavender");
        adjust(&mut c, ThemePreset, 1);
        assert_eq!(value(&c, ThemePreset), "catppuccin");
        assert_eq!(value(&c, VizGradient), "catppuccin");
        adjust(&mut c, ThemePreset, -1);
        adjust(&mut c, ThemePreset, -1);
        assert_eq!(value(&c, ThemePreset), "terminal"); // wraps

        c.artwork.size = 0.9;
        adjust(&mut c, ArtSize, 1);
        assert_eq!(c.artwork.size, 0.9); // clamps
        c.visualizer.spacing = 0;
        adjust(&mut c, VizSpacing, -1);
        assert_eq!(c.visualizer.spacing, 0);
        assert_eq!(adjust(&mut c, VizBars, 1), Effect::RestartVisualizer);

        assert!(set_text(&mut c, Accent, "#123456").is_ok());
        assert!(set_text(&mut c, Accent, "nope").is_err());
        assert!(set_text(&mut c, VizColors, "accent").is_ok());
        adjust(&mut c, ThemePreset, 1);
        assert_eq!(c.visualizer.colors, ["accent"]); // follows theme, untouched
        set_text(&mut c, VizColors, "#111111, red").unwrap();
        assert_eq!(c.visualizer.colors.len(), 2);
        assert!(set_text(&mut c, Background, "").is_ok());
        assert_eq!(value(&c, Background), "terminal");

        set_text(&mut c, PreferredPlayer, "spotify").unwrap();
        assert_eq!(c.preferred_player.as_deref(), Some("spotify"));
        set_text(&mut c, PreferredPlayer, "").unwrap();
        assert_eq!(value(&c, PreferredPlayer), "none");

        adjust(&mut c, VizBarWidth, -1);
        assert_eq!(value(&c, VizBarWidth), "auto");
        adjust(&mut c, ArtPosition, 1);
        assert_eq!(value(&c, ArtPosition), "top");
        assert_eq!(reset(&mut c, ArtPosition), Effect::None);
        assert_eq!(value(&c, ArtPosition), "auto");
        c.visualizer.bars = 100;
        assert_eq!(reset(&mut c, VizBars), Effect::RestartVisualizer);
        assert_eq!(c.visualizer.bars, 48);

        // Every field has a label and help text, and appears in exactly one tab.
        let fields: Vec<Field> = TABS.iter().flat_map(|(_, f)| f.iter().copied()).collect();
        assert_eq!(fields.len(), 33);
        for f in &fields {
            assert!(!label(*f).is_empty() && !help(*f).is_empty());
            assert_eq!(fields.iter().filter(|g| g == &f).count(), 1);
        }
    }
}
