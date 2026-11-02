//! Layout and rendering: a centered artwork-first composition with optional overlays.

mod visualizer;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Margin, Rect, Size};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Padding, Paragraph};
use ratatui_image::{Resize, StatefulImage};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{App, Overlay};
use crate::config::color;
use crate::config::settings::{self, SECTIONS};
use crate::media::fmt_time;
use crate::mpris::{Player, Status};

struct Palette {
    accent: Color,
    fg: Color,
    dim: Color,
    bg: Color,
}

impl Palette {
    /// Text color on an accent background.
    fn on_accent(&self) -> Color {
        if self.bg == Color::Reset { Color::Black } else { self.bg }
    }
}

struct Glyphs {
    prev: &'static str,
    play: &'static str,
    pause: &'static str,
    next: &'static str,
    stopped: &'static str,
    filled: &'static str,
    empty: &'static str,
    knob: &'static str,
    dot: &'static str,
    bullet: &'static str,
}

const UNICODE: Glyphs = Glyphs {
    prev: "⏮",
    play: "▶",
    pause: "⏸",
    next: "⏭",
    stopped: "■",
    filled: "━",
    empty: "─",
    knob: "●",
    dot: "·",
    bullet: "●",
};
const ASCII: Glyphs = Glyphs {
    prev: "|<",
    play: ">",
    pause: "||",
    next: ">|",
    stopped: "[]",
    filled: "=",
    empty: "-",
    knob: "o",
    dot: "-",
    bullet: "*",
};

/// Width of the settings drawer when open.
const DRAWER_W: u16 = 50;

pub fn draw(f: &mut Frame, app: &mut App) {
    let t = &app.cfg.theme;
    let pal = Palette {
        accent: app.accent(),
        fg: color(&t.foreground),
        dim: color(&t.dim),
        bg: if t.background.is_empty() { Color::Reset } else { color(&t.background) },
    };
    let g = if app.cfg.ascii { &ASCII } else { &UNICODE };
    let mut area = f.area();
    f.render_widget(Block::new().style(Style::new().bg(pal.bg)), area);

    // The settings drawer takes the right side so changes preview live on the left.
    if let Overlay::Settings { .. } = app.overlay {
        let w = DRAWER_W.min(area.width);
        let drawer = Rect { x: area.right() - w, width: w, ..area };
        draw_settings(f, app, drawer, &pal);
        area.width -= w;
    }

    let [body, footer] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);
    draw_footer(f, app, footer, &pal, g);

    // Share the height between the visualizer and the player, always keeping room for the
    // compact info block so short terminals still get both.
    let viz_h = (body.height.saturating_sub(10) / 2)
        .max(body.height / 3)
        .min(app.cfg.visualizer.height)
        .min(body.height.saturating_sub(COMPACT_INFO_H));
    let [main, viz] = Layout::vertical([Constraint::Fill(1), Constraint::Length(viz_h)]).areas(body);
    if viz_h > 0 && app.cfg.visualizer.enabled {
        let gradient = gradient(&app.cfg.visualizer.colors, pal.accent);
        let viz = viz.inner(Margin::new(viz.width.min(4), 0));
        let spectrum = visualizer::Spectrum {
            values: &app.spectrum,
            mode: app.mode(),
            spacing: app.cfg.visualizer.spacing,
            gradient: &gradient,
            ascii: app.cfg.ascii,
        };
        f.render_widget(spectrum, viz);
    }

    match app.current().cloned() {
        None => draw_empty(f, app, main, &pal),
        Some(p) => draw_player(f, app, &p, main, &pal, g),
    }

    match app.overlay {
        Overlay::None | Overlay::Settings { .. } => {}
        Overlay::Help => draw_help(f, app, area, &pal),
        Overlay::Selector(i) => draw_selector(f, app, i, area, &pal, g),
    }
}

/// Resolves configured gradient stops; `"accent"` expands to a ramp around the accent.
fn gradient(colors: &[String], accent: Color) -> Vec<Color> {
    if colors.len() == 1 && colors[0] == "accent" {
        let black = Color::Rgb(0, 0, 0);
        let white = Color::Rgb(255, 255, 255);
        return vec![visualizer::lerp(accent, black, 0.55), accent, visualizer::lerp(accent, white, 0.45)];
    }
    colors.iter().map(|c| if c == "accent" { accent } else { color(c) }).collect()
}

/// Cuts `s` to at most `width` terminal columns (CJK counts double), adding "…" if cut.
fn fit(s: &str, width: usize) -> String {
    if s.width() <= width {
        return s.to_string();
    }
    let mut out = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = c.width().unwrap_or(0);
        if w + cw + 1 > width {
            break;
        }
        out.push(c);
        w += cw;
    }
    out.push('…');
    out
}

fn draw_empty(f: &mut Frame, app: &App, area: Rect, pal: &Palette) {
    let msg = if app.players.is_empty() { "No media players found" } else { "Nothing selected" };
    let lines = vec![
        Line::styled(msg, Style::new().fg(pal.fg).add_modifier(Modifier::BOLD)),
        Line::styled("Start playback in any MPRIS-capable app", Style::new().fg(pal.dim)),
    ];
    let [mid] = Layout::vertical([Constraint::Length(2)]).flex(Flex::Center).areas(area);
    f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), mid);
}

/// Rows used by the text block: title, artist, album, gap, progress, gap, controls.
const INFO_H: u16 = 7;
/// Rows used on short terminals: title, artist, progress, controls.
const COMPACT_INFO_H: u16 = 4;

fn draw_player(f: &mut Frame, app: &mut App, p: &Player, area: Rect, pal: &Palette, g: &Glyphs) {
    let landscape = area.width > area.height * 4 && area.height >= INFO_H;
    let show_art = app.cfg.artwork.show && app.art.is_some();

    if landscape && show_art {
        // Wide, short terminals: artwork left, details right.
        let art_h = area.height.saturating_sub(2);
        let art_w = (art_h * 2).min(area.width / 2);
        let [_, art_col, _, info_col, _] = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Length(art_w),
            Constraint::Length(4),
            Constraint::Length(area.width.saturating_sub(art_w + 8).min(60)),
            Constraint::Fill(1),
        ])
        .areas(area);
        draw_art(f, app, art_col.inner(Margin::new(0, 1)));
        let [info] = Layout::vertical([Constraint::Length(INFO_H)]).flex(Flex::Center).areas(info_col);
        draw_info(f, p, info, pal, g, Alignment::Left, false);
        return;
    }

    let compact = area.height < INFO_H;
    let info_h = if compact { COMPACT_INFO_H } else { INFO_H };
    let size = app.cfg.artwork.size.clamp(0.1, 0.9);
    let art_h = if show_art {
        ((area.height as f32 * size) as u16).min(area.height.saturating_sub(info_h + 2)).min(area.width / 2)
    } else {
        0
    };
    let gap = u16::from(art_h > 0) * 2;
    let [_, art, _, info, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(art_h),
        Constraint::Length(gap),
        Constraint::Length(info_h),
        Constraint::Fill(1),
    ])
    .areas(area);
    if art_h > 0 {
        draw_art(f, app, art);
    }
    let w = info.width.min(64);
    let info = Rect { x: info.x + (info.width - w) / 2, width: w, ..info };
    draw_info(f, p, info, pal, g, Alignment::Center, compact);
}

/// Renders the artwork centered in `area`, keeping its aspect ratio.
fn draw_art(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(art) = app.art.as_mut() else { return };
    let size = art.size_for(Resize::Fit(None), Size::new(area.width, area.height)).unwrap_or(area.as_size());
    let rect = Rect {
        x: area.x + area.width.saturating_sub(size.width) / 2,
        y: area.y + area.height.saturating_sub(size.height) / 2,
        width: size.width.min(area.width),
        height: size.height.min(area.height),
    };
    f.render_stateful_widget(StatefulImage::default(), rect, art);
}

/// Draws title, artist, progress and controls; `compact` drops the album and spacer rows.
fn draw_info(
    f: &mut Frame,
    p: &Player,
    area: Rect,
    pal: &Palette,
    g: &Glyphs,
    align: Alignment,
    compact: bool,
) {
    let w = area.width as usize;
    let title = if p.title.is_empty() { "Unknown title" } else { &p.title };
    let pos = p.position_now();

    let mut lines = vec![
        Line::styled(fit(title, w), Style::new().fg(pal.fg).add_modifier(Modifier::BOLD)),
        Line::styled(fit(&p.artists.join(", "), w), Style::new().fg(pal.accent)),
    ];
    if !compact {
        lines.push(Line::styled(fit(&p.album, w), Style::new().fg(pal.dim)));
        lines.push(Line::raw(""));
    }

    match p.length {
        // "1:23 ━━━━●──── 3:45"
        Some(len) => {
            let (l, r) = (fmt_time(pos), fmt_time(len));
            let bar_w = w.saturating_sub(l.len() + r.len() + 2);
            let ratio = (pos.as_secs_f64() / len.as_secs_f64().max(0.001)).clamp(0.0, 1.0);
            let done = ((bar_w as f64 * ratio) as usize).min(bar_w.saturating_sub(1));
            lines.push(Line::from(vec![
                Span::styled(format!("{l} "), Style::new().fg(pal.dim)),
                Span::styled(g.filled.repeat(done), Style::new().fg(pal.accent)),
                Span::styled(g.knob, Style::new().fg(pal.fg)),
                Span::styled(g.empty.repeat(bar_w.saturating_sub(done + 1)), Style::new().fg(pal.dim)),
                Span::styled(format!(" {r}"), Style::new().fg(pal.dim)),
            ]));
        }
        // Length unknown but the player does report position (e.g. a stream): elapsed only.
        None if !p.position.is_zero() => lines.push(Line::from(vec![
            Span::styled(fmt_time(pos), Style::new().fg(pal.fg)),
            Span::styled(format!(" {} live", g.dot), Style::new().fg(pal.dim)),
        ])),
        // Neither: the player (often a browser tab) exposes no progress; don't invent one.
        None => lines.push(Line::styled(
            fit(&format!("{} doesn't share track progress", p.identity), w),
            Style::new().fg(pal.dim).add_modifier(Modifier::ITALIC),
        )),
    }
    if !compact {
        lines.push(Line::raw(""));
    }

    let c = p.caps;
    let on = |enabled: bool, s: Style| {
        if enabled && c.control { s } else { Style::new().fg(pal.dim).add_modifier(Modifier::DIM) }
    };
    let toggle = if p.status == Status::Playing { g.pause } else { g.play };
    lines.push(Line::from(vec![
        Span::styled(g.prev, on(c.prev, Style::new().fg(pal.fg))),
        Span::raw("     "),
        Span::styled(toggle, on(c.play || c.pause, Style::new().fg(pal.accent).add_modifier(Modifier::BOLD))),
        Span::raw("     "),
        Span::styled(g.next, on(c.next, Style::new().fg(pal.fg))),
    ]));
    f.render_widget(Paragraph::new(lines).alignment(align), area);
}

fn status_label(s: Status) -> &'static str {
    match s {
        Status::Playing => "playing",
        Status::Paused => "paused",
        Status::Stopped => "stopped",
    }
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect, pal: &Palette, g: &Glyphs) {
    let mut left = vec![Span::raw("  ")];
    if let Some(p) = app.current() {
        let dot = Style::new().fg(if p.status == Status::Playing { pal.accent } else { pal.dim });
        left.push(Span::styled(g.bullet, dot));
        left.push(Span::styled(format!(" {} ", p.identity), Style::new().fg(pal.fg)));
        left.push(Span::styled(format!("{} {}", g.dot, status_label(p.status)), Style::new().fg(pal.dim)));
        if app.players.len() > 1 {
            left.push(Span::styled(
                format!("  {} {} players", g.dot, app.players.len()),
                Style::new().fg(pal.dim),
            ));
        }
    }
    if let Some(reason) = app.viz_fallback.as_deref().filter(|_| app.cfg.visualizer.enabled) {
        left.push(Span::styled(format!("  {} no audio: {reason}", g.dot), Style::new().fg(pal.dim)));
    }
    let right = match app.status() {
        Some(s) => Span::styled(format!("{s}  "), Style::new().fg(pal.accent)),
        None => Span::styled("? help  o settings  ", Style::new().fg(pal.dim)),
    };
    let rw = (right.width() as u16).min(area.width);
    let [l, r] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(rw)]).areas(area);
    f.render_widget(Line::from(left), l);
    f.render_widget(right, r);
}

fn popup(area: Rect, w: u16, h: u16) -> Rect {
    let (w, h) = (w.min(area.width), h.min(area.height));
    Rect { x: area.x + (area.width - w) / 2, y: area.y + (area.height - h) / 2, width: w, height: h }
}

fn frame_block<'a>(title: &str, pal: &Palette) -> Block<'a> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(pal.dim))
        .title(Span::styled(format!(" {title} "), Style::new().fg(pal.accent).add_modifier(Modifier::BOLD)))
        .style(Style::new().bg(pal.bg))
}

fn hint(text: &str, pal: &Palette) -> Line<'static> {
    Line::styled(format!(" {text} "), Style::new().fg(pal.dim))
}

fn draw_settings(f: &mut Frame, app: &App, area: Rect, pal: &Palette) {
    let Overlay::Settings { sel, edit } = &app.overlay else { return };
    let inner_w = area.width.saturating_sub(4) as usize;
    let label_w = 20.min(inner_w / 2);
    let mut lines: Vec<Line> = Vec::new();
    let mut sel_line = 0;
    let mut idx = 0;
    for (section, fields) in SECTIONS {
        if !lines.is_empty() {
            lines.push(Line::raw(""));
        }
        lines.push(Line::styled(
            section.to_uppercase(),
            Style::new().fg(pal.dim).add_modifier(Modifier::BOLD),
        ));
        for &field in *fields {
            let selected = idx == *sel;
            if selected {
                sel_line = lines.len();
            }
            let value = match edit {
                Some(buf) if selected => format!("{buf}▏"),
                _ if selected && !settings::is_text(field) => {
                    format!("‹ {} ›", settings::value(&app.cfg, field))
                }
                _ => settings::value(&app.cfg, field),
            };
            let value = fit(&value, inner_w.saturating_sub(label_w + 3));
            let swatch = swatch_for(app, field);
            let base = if selected {
                Style::new().fg(pal.on_accent()).bg(pal.accent)
            } else {
                Style::new().fg(pal.fg)
            };
            let pad = inner_w.saturating_sub(label_w + 2 + value.width() + swatch.is_some() as usize * 2);
            let mut spans = vec![
                Span::styled(format!(" {:<w$}", settings::label(field), w = label_w), base),
                Span::styled(" ".repeat(pad), base),
            ];
            if let Some(c) = swatch {
                spans.push(Span::styled("■ ", base.fg(c)));
            }
            spans.push(Span::styled(
                format!("{value} "),
                if selected { base } else { Style::new().fg(pal.accent) },
            ));
            lines.push(Line::from(spans));
            idx += 1;
        }
    }

    // Scroll so the selected row stays visible.
    let visible = area.height.saturating_sub(4) as usize;
    let scroll = sel_line.saturating_sub(visible.saturating_sub(3));
    let title = if app.dirty { "settings •" } else { "settings" };
    let help = if edit.is_some() {
        "type · enter apply · esc cancel"
    } else {
        "←→ change · enter edit · s save · r revert"
    };
    let block = frame_block(title, pal).title_bottom(hint(help, pal)).padding(Padding::uniform(1));
    f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(lines).block(block).scroll((scroll as u16, 0)), area);
}

/// Color preview next to color fields.
fn swatch_for(app: &App, field: settings::Field) -> Option<Color> {
    use settings::Field::{Accent, AccentFromArt, Background, Dim, Foreground};
    let t = &app.cfg.theme;
    let c = match field {
        Accent => color(&t.accent),
        Foreground => color(&t.foreground),
        Dim => color(&t.dim),
        Background if !t.background.is_empty() => color(&t.background),
        AccentFromArt if t.accent_from_artwork => app.art_accent?,
        _ => return None,
    };
    (c != Color::Reset).then_some(c)
}

fn draw_selector(f: &mut Frame, app: &App, sel: usize, area: Rect, pal: &Palette, g: &Glyphs) {
    let rect_w = 60.min(area.width);
    let inner_w = rect_w.saturating_sub(4) as usize;
    let lines: Vec<Line> = if app.players.is_empty() {
        vec![Line::styled("no players", Style::new().fg(pal.dim))]
    } else {
        app.players
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let icon = match p.status {
                    Status::Playing => g.play,
                    Status::Paused => g.pause,
                    Status::Stopped => g.stopped,
                };
                let current = app.current().is_some_and(|c| c.bus == p.bus);
                let base = if i == sel {
                    Style::new().fg(pal.on_accent()).bg(pal.accent)
                } else {
                    Style::new().fg(pal.fg)
                };
                let mark = if current { if app.cfg.ascii { ">" } else { "›" } } else { " " };
                let name = fit(&p.identity, 16);
                let title = if p.title.is_empty() { "—" } else { &p.title };
                let rest = inner_w.saturating_sub(name.width().max(16) + 5);
                Line::from(vec![
                    Span::styled(format!("{mark}{icon} "), base),
                    Span::styled(format!("{name:<16}"), base.add_modifier(Modifier::BOLD)),
                    Span::styled(format!(" {:<rest$} ", fit(title, rest)), base),
                ])
            })
            .collect()
    };
    let rect = popup(area, rect_w, lines.len() as u16 + 4);
    f.render_widget(Clear, rect);
    let block = frame_block("players", pal).title_bottom(hint("↑↓ select · enter choose · esc close", pal));
    f.render_widget(Paragraph::new(lines).block(block.padding(Padding::uniform(1))), rect);
}

fn draw_help(f: &mut Frame, app: &App, area: Rect, pal: &Palette) {
    let k = &app.cfg.keys;
    let rows = [
        ("play / pause", &k.play_pause),
        ("next", &k.next),
        ("previous", &k.previous),
        ("stop", &k.stop),
        ("seek forward", &k.seek_forward),
        ("seek backward", &k.seek_backward),
        ("player selector", &k.player_selector),
        ("next player", &k.next_player),
        ("visualizer style", &k.visualizer_mode),
        ("settings", &k.settings),
        ("quit", &k.quit),
    ];
    let lines: Vec<Line> = rows
        .iter()
        .map(|(name, keys)| {
            Line::from(vec![
                Span::styled(format!("{name:<18}"), Style::new().fg(pal.fg)),
                Span::styled(keys.join(" / "), Style::new().fg(pal.accent)),
            ])
        })
        .collect();
    let rect = popup(area, 44, lines.len() as u16 + 4);
    f.render_widget(Clear, rect);
    let block = frame_block("keys", pal).padding(Padding::uniform(1));
    f.render_widget(Paragraph::new(lines).block(block), rect);
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use ratatui::crossterm::event::{Event as TermEvent, KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend};

    use crate::app::{App, Event};
    use crate::mpris::{Caps, Player, Status, Update};

    fn text(term: &Terminal<TestBackend>) -> String {
        term.backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect()
    }

    fn key(app: &mut App, code: KeyCode) {
        app.handle(Event::Term(TermEvent::Key(KeyEvent::new(code, KeyModifiers::NONE))));
    }

    #[tokio::test]
    async fn renders_player_and_settings_at_any_size() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let (rtx, _rrx) = tokio::sync::mpsc::unbounded_channel();
        let mut cfg = crate::config::Config::default();
        cfg.visualizer.enabled = false; // don't spawn cava in tests
        let picker = ratatui_image::picker::Picker::halfblocks();
        let mut app = App::new(cfg, Some("/nonexistent/utatui.toml".into()), None, tx, picker, rtx);
        let mut player = Player {
            bus: "org.mpris.MediaPlayer2.test".into(),
            identity: "Test".into(),
            status: Status::Playing,
            title: "少女レイ / みきとP - Covered by しほ and a very long tail".into(),
            artists: vec!["Artist".into()],
            album: "Album".into(),
            art_url: None,
            length: Some(Duration::from_secs(200)),
            position: Duration::from_secs(50),
            position_at: Instant::now(),
            rate: 1.0,
            caps: Caps { control: true, play: true, pause: true, next: true, prev: false, seek: true },
        };
        app.handle(Event::Player(Update::Changed(player.clone())));
        app.handle(Event::Spectrum((0..48).map(|i| i as f32 / 48.0).collect()));

        let mut term = Terminal::new(TestBackend::new(100, 40)).unwrap();
        term.draw(|f| super::draw(f, &mut app)).unwrap();
        assert!(text(&term).contains("Artist") && text(&term).contains("3:20"));

        // Settings drawer opens, edits live, and every size still renders.
        key(&mut app, KeyCode::Char('o'));
        key(&mut app, KeyCode::Right); // preset -> catppuccin
        assert_eq!(app.cfg.theme.accent, "#cba6f7");
        assert!(app.dirty);
        for (w, h) in [(100, 40), (200, 30), (60, 20), (30, 8), (1, 1)] {
            let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
            term.draw(|f| super::draw(f, &mut app)).unwrap();
            if w == 100 {
                assert!(text(&term).contains("catppuccin"));
                if std::env::var_os("SHOW").is_some() {
                    println!("{}", term.backend());
                }
            }
        }
        key(&mut app, KeyCode::Esc);

        // No length and no position: say so instead of showing a timer.
        player.length = None;
        player.position = Duration::ZERO;
        app.handle(Event::Player(Update::Changed(player)));
        let mut term = Terminal::new(TestBackend::new(100, 40)).unwrap();
        term.draw(|f| super::draw(f, &mut app)).unwrap();
        assert!(text(&term).contains("doesn't share track progress"));
    }

    /// Short terminals keep both the visualizer and the essential info.
    #[tokio::test]
    async fn short_terminal_shows_visualizer() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let (rtx, _rrx) = tokio::sync::mpsc::unbounded_channel();
        let picker = ratatui_image::picker::Picker::halfblocks();
        let mut app = App::new(crate::config::Config::default(), None, None, tx, picker, rtx);
        app.handle(Event::Player(Update::Changed(Player {
            bus: "org.mpris.MediaPlayer2.test".into(),
            identity: "Test".into(),
            status: Status::Playing,
            title: "Song".into(),
            artists: vec!["Artist".into()],
            album: "Album".into(),
            art_url: None,
            length: Some(Duration::from_secs(200)),
            position: Duration::from_secs(50),
            position_at: Instant::now(),
            rate: 1.0,
            caps: Caps { control: true, play: true, pause: true, next: true, prev: true, seek: true },
        })));
        app.handle(Event::Spectrum(vec![1.0; 48]));
        for (w, h) in [(65, 10), (65, 8), (40, 6)] {
            let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
            term.draw(|f| super::draw(f, &mut app)).unwrap();
            let screen = text(&term);
            assert!(screen.contains('█'), "no visualizer at {w}x{h}");
            assert!(screen.contains("Song") && screen.contains("3:20"), "info missing at {w}x{h}");
            if std::env::var_os("SHOW").is_some() {
                println!("{}", term.backend());
            }
        }
    }

    #[test]
    fn fit_counts_wide_chars() {
        assert_eq!(super::fit("abc", 5), "abc");
        assert_eq!(super::fit("abcdef", 4), "abc…");
        assert_eq!(super::fit("少女レイ", 5), "少女…");
    }
}
