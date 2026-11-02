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
    Ascii,
    SeekStep,
    ArtShow,
    ArtSize,
    ArtProtocol,
    VizEnabled,
    VizMode,
    VizGradient,
    VizColors,
    VizHeight,
    VizBars,
    VizSpacing,
    VizSensitivity,
    VizFramerate,
    VizSmoothing,
}

use Field::{
    Accent, AccentFromArt, ArtProtocol, ArtShow, ArtSize, Ascii, Background, Dim, Foreground, SeekStep,
    ThemePreset, VizBars, VizColors, VizEnabled, VizFramerate, VizGradient, VizHeight, VizMode,
    VizSensitivity, VizSmoothing, VizSpacing,
};

pub const SECTIONS: &[(&str, &[Field])] = &[
    ("Theme", &[ThemePreset, AccentFromArt, Accent, Foreground, Dim, Background]),
    ("Artwork", &[ArtShow, ArtSize, ArtProtocol]),
    (
        "Visualizer",
        &[
            VizEnabled,
            VizMode,
            VizGradient,
            VizColors,
            VizHeight,
            VizBars,
            VizSpacing,
            VizSensitivity,
            VizFramerate,
            VizSmoothing,
        ],
    ),
    ("General", &[SeekStep, Ascii]),
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

pub fn all() -> impl Iterator<Item = Field> {
    SECTIONS.iter().flat_map(|(_, f)| f.iter().copied())
}

pub fn label(f: Field) -> &'static str {
    match f {
        ThemePreset => "Preset",
        AccentFromArt => "Accent from artwork",
        Accent => "Accent",
        Foreground => "Text",
        Dim => "Muted text",
        Background => "Background",
        Ascii => "ASCII glyphs",
        SeekStep => "Seek step",
        ArtShow => "Show artwork",
        ArtSize => "Size",
        ArtProtocol => "Protocol",
        VizEnabled => "Enabled",
        VizMode => "Style",
        VizGradient => "Gradient",
        VizColors => "Colors",
        VizHeight => "Height",
        VizBars => "Bars",
        VizSpacing => "Spacing",
        VizSensitivity => "Sensitivity",
        VizFramerate => "Frame rate",
        VizSmoothing => "Smoothing",
    }
}

/// Fields edited by typing (colors) rather than stepping.
pub fn is_text(f: Field) -> bool {
    matches!(f, Accent | Foreground | Dim | Background | VizColors)
}

fn on_off(b: bool) -> String {
    if b { "on" } else { "off" }.into()
}

pub fn value(c: &Config, f: Field) -> String {
    let (t, a, v) = (&c.theme, &c.artwork, &c.visualizer);
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
        Ascii => on_off(c.ascii),
        SeekStep => format!("{}s", c.seek_step),
        ArtShow => on_off(a.show),
        ArtSize => format!("{:.0}%", a.size * 100.0),
        ArtProtocol => a.protocol.clone(),
        VizEnabled => on_off(v.enabled),
        VizMode => format!("{:?}", v.mode).to_lowercase(),
        VizGradient => gradient_name(&v.colors).into(),
        VizColors => v.colors.join(", "),
        VizHeight => format!("{} rows", v.height),
        VizBars => v.bars.to_string(),
        VizSpacing => v.spacing.to_string(),
        VizSensitivity => format!("{}%", v.sensitivity),
        VizFramerate => format!("{} fps", v.framerate),
        VizSmoothing => v.noise_reduction.to_string(),
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

/// Steps a non-text field left (`dir < 0`) or right (`dir > 0`).
pub fn adjust(c: &mut Config, f: Field, dir: i32) -> Effect {
    let (t, a, v) = (&mut c.theme, &mut c.artwork, &mut c.visualizer);
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
        AccentFromArt => t.accent_from_artwork = !t.accent_from_artwork,
        Ascii => c.ascii = !c.ascii,
        SeekStep => c.seek_step = step(c.seek_step, dir, 1, 1, 60),
        ArtShow => a.show = !a.show,
        ArtSize => a.size = (step(a.size, dir, 0.05, 0.15, 0.9) * 100.0).round() / 100.0,
        ArtProtocol => {
            a.protocol = cycle(PROTOCOLS, a.protocol.as_str(), dir).into();
            return Effect::NextLaunch;
        }
        VizEnabled => {
            v.enabled = !v.enabled;
            return Effect::RestartVisualizer;
        }
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
        VizBars => {
            v.bars = step(v.bars, dir, 4, 4, 256);
            return Effect::RestartVisualizer;
        }
        VizSpacing => v.spacing = step(v.spacing, dir, 1, 0, 4),
        VizSensitivity => {
            v.sensitivity = step(v.sensitivity, dir, 10, 10, 500);
            return Effect::RestartVisualizer;
        }
        VizFramerate => {
            v.framerate = cycle(FRAMERATES, v.framerate, dir);
            return Effect::RestartVisualizer;
        }
        VizSmoothing => {
            v.noise_reduction = step(v.noise_reduction, dir, 5, 0, 100);
            return Effect::RestartVisualizer;
        }
        Accent | Foreground | Dim | Background | VizColors => {}
    }
    Effect::None
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
        assert_eq!(all().count(), 21);
    }
}
