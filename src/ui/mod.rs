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
use crate::config::settings::{self, TABS};
use crate::config::{ArtPosition, color};
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

/// Width of the settings drawer when the terminal is big enough to keep a live preview.
const DRAWER_W: u16 = 52;
/// Below this size the player switches to the mini layout.
const MINI_BELOW: (u16, u16) = (40, 8);

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

    // Big terminals get a side drawer with a live preview; smaller ones a full-screen page.
    if let Overlay::Settings { .. } = app.overlay {
        if area.width >= 100 && area.height >= 20 {
            let drawer = Rect { x: area.right() - DRAWER_W, width: DRAWER_W, ..area };
            draw_settings(f, app, drawer, &pal);
            area.width -= DRAWER_W;
        } else {
            draw_settings(f, app, area, &pal);
            return;
        }
    }

    let footer_h = u16::from(app.cfg.layout.show_footer && area.height >= 8);
    let [body, footer] = Layout::vertical([Constraint::Fill(1), Constraint::Length(footer_h)]).areas(area);
    if footer_h > 0 {
        draw_footer(f, app, footer, &pal, g);
    }

    match app.current().cloned() {
        None => draw_empty(f, app, body, &pal),
        Some(p) if body.width < MINI_BELOW.0 || body.height < MINI_BELOW.1 => {
            draw_mini(f, app, &p, body, &pal, g);
        }
        Some(p) => draw_full(f, app, &p, body, &pal, g),
    }

    match app.overlay {
        Overlay::None | Overlay::Settings { .. } => {}
        Overlay::Help => draw_help(f, app, area, &pal),
        Overlay::Selector(i) => draw_selector(f, app, i, area, &pal, g),
    }
}

fn draw_spectrum(f: &mut Frame, app: &App, area: Rect, pal: &Palette) {
    if area.height == 0 || area.width == 0 || !app.cfg.visualizer.enabled {
        return;
    }
    let v = &app.cfg.visualizer;
    let gradient = gradient(&v.colors, pal.accent);
    let spectrum = visualizer::Spectrum {
        values: &app.spectrum,
        mode: app.mode(),
        // Config values aren't range-checked; keep the bar math within the area.
        spacing: v.spacing.min(area.width),
        bar_width: v.bar_width.min(area.width),
        baseline: v.baseline,
        gradient: &gradient,
        ascii: app.cfg.ascii,
    };
    f.render_widget(spectrum, area);
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

/// Shows `s` in `width` columns, scrolling it sideways when it doesn't fit.
fn marquee(s: &str, width: usize, seconds: f32) -> String {
    const GAP: &str = "   ·   ";
    const COLS_PER_SECOND: f32 = 5.0;
    if s.width() <= width {
        return s.to_string();
    }
    let looped = format!("{s}{GAP}");
    let offset = (seconds * COLS_PER_SECOND) as usize % looped.width();
    let mut out = String::new();
    let (mut col, mut w) = (0, 0);
    for c in looped.chars().chain(looped.chars()) {
        let cw = c.width().unwrap_or(0);
        if col < offset {
            col += cw;
            // A wide character cut by the left edge leaves a blank column.
            if col > offset {
                out.push(' ');
                w += 1;
            }
            continue;
        }
        if w + cw > width {
            break;
        }
        out.push(c);
        w += cw;
    }
    out
}

fn title_text(app: &App, p: &Player, width: usize) -> String {
    let title = if p.title.is_empty() { "Unknown title" } else { &p.title };
    if app.cfg.layout.scroll_title {
        marquee(title, width, app.started.elapsed().as_secs_f32())
    } else {
        fit(title, width)
    }
}

fn draw_empty(f: &mut Frame, app: &App, area: Rect, pal: &Palette) {
    let w = area.width as usize;
    let msg = if app.players.is_empty() { "No media players found" } else { "Nothing selected" };
    let mut lines = vec![Line::styled(fit(msg, w), Style::new().fg(pal.fg).add_modifier(Modifier::BOLD))];
    if area.height >= 3 {
        lines.push(Line::styled(fit("Start playback in any MPRIS app", w), Style::new().fg(pal.dim)));
    }
    let [mid] = Layout::vertical([Constraint::Length(lines.len() as u16)]).flex(Flex::Center).areas(area);
    f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), mid);
}

/// Tiny terminals: artwork on the left at full height; title, progress and the visualizer on
/// the right. Artwork and visualizer are kept at every size; text gives way first.
fn draw_mini(f: &mut Frame, app: &mut App, p: &Player, area: Rect, pal: &Palette, g: &Glyphs) {
    let viz_on = app.cfg.visualizer.enabled;
    let show_art = app.cfg.artwork.show && app.art.is_some() && area.width >= 10;
    let mut right = area;
    if show_art {
        let art_w = (area.height * 2).min(area.width * 2 / 5).max(2);
        let [art, _, rest] =
            Layout::horizontal([Constraint::Length(art_w), Constraint::Length(1), Constraint::Fill(1)])
                .areas(area);
        draw_art(f, app, art);
        right = rest;
    }

    // Rows each line needs before it appears; with the visualizer off, text takes everything.
    let h = right.height;
    let need = |with_viz: u16, without: u16| if viz_on { h >= with_viz } else { h >= without };
    let w = right.width as usize;
    let mut lines = Vec::new();
    if need(2, 1) {
        lines.push(Line::styled(title_text(app, p, w), Style::new().fg(pal.fg).add_modifier(Modifier::BOLD)));
    }
    if need(6, 3) && !p.artists.is_empty() {
        lines.push(Line::styled(fit(&p.artists.join(", "), w), Style::new().fg(pal.accent)));
    }
    if need(4, 2)
        && let Some(progress) = progress_line(app, p, w, pal, g)
    {
        lines.push(progress);
    }
    if need(7, 4) && app.cfg.layout.show_controls {
        lines.push(controls_line(p, pal, g));
    }
    let text_h = (lines.len() as u16).min(h);
    let [text, viz] = Layout::vertical([Constraint::Length(text_h), Constraint::Fill(1)]).areas(right);
    f.render_widget(Paragraph::new(lines), text);
    draw_spectrum(f, app, viz, pal);
}

/// Regular terminals. Space is handed out by priority: the cover first, then the visualizer,
/// then the text, which shrinks from a full card down to just the title to fit what is left.
fn draw_full(f: &mut Frame, app: &mut App, p: &Player, body: Rect, pal: &Palette, g: &Glyphs) {
    let viz_h =
        if app.cfg.visualizer.enabled { app.cfg.visualizer.height.min((body.height / 4).max(2)) } else { 0 };
    let [main, viz] = Layout::vertical([Constraint::Fill(1), Constraint::Length(viz_h)]).areas(body);
    draw_spectrum(f, app, viz.inner(Margin::new(viz.width.min(4), 0)), pal);

    let show_art = app.cfg.artwork.show && app.art.is_some();
    let share = app.cfg.artwork.size.clamp(0.1, 0.9);
    // Stacked: the cover takes its share of the screen, keeping a gap and one row of text.
    let stacked_h = ((body.height as f32 * share) as u16).min(main.height.saturating_sub(2));
    let stacked =
        if show_art { art_size(app, Size::new(main.width.saturating_sub(4), stacked_h)) } else { Size::ZERO };
    // Side by side: the cover can use the full height of the player area.
    let beside_h = (main.height as f32 * (share * 2.0).min(1.0)) as u16;
    let beside = if show_art { art_size(app, Size::new(main.width * 2 / 5, beside_h)) } else { Size::ZERO };

    let side_by_side = show_art
        && match app.cfg.artwork.position {
            ArtPosition::Side => true,
            ArtPosition::Top => false,
            // Beside the text when that makes the cover clearly bigger, as on short terminals.
            ArtPosition::Auto => {
                main.width >= 60 && u32::from(beside.height) * 3 > u32::from(stacked.height) * 4
            }
        };
    if side_by_side {
        draw_side_by_side(f, app, p, main, beside, pal, g);
        return;
    }

    let gap = u16::from(stacked.height > 0);
    let card_w = (stacked.width + 8).clamp(40, 72).min(main.width);
    let (lines, card) =
        fitted_info(app, p, card_w as usize, main.height.saturating_sub(stacked.height + gap), pal, g);
    let info_h = lines.len() as u16 + 2 * u16::from(card);
    let top = main.y + main.height.saturating_sub(stacked.height + gap + info_h) / 2;
    if stacked.height > 0 {
        let art = Rect {
            x: main.x + (main.width - stacked.width) / 2,
            y: top,
            width: stacked.width,
            height: stacked.height,
        };
        draw_art(f, app, art);
    }
    let info = Rect {
        x: main.x + (main.width - card_w) / 2,
        y: top + stacked.height + gap,
        width: card_w,
        height: info_h.min(main.height),
    };
    render_info(f, info, lines, card, pal, Alignment::Center);
}

/// Short or wide terminals: cover on the left, text on the right.
fn draw_side_by_side(
    f: &mut Frame,
    app: &mut App,
    p: &Player,
    main: Rect,
    art: Size,
    pal: &Palette,
    g: &Glyphs,
) {
    let text_w = main.width.saturating_sub(art.width + 7).min(60);
    let [_, art_col, _, info_col, _] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(art.width),
        Constraint::Length(3),
        Constraint::Length(text_w),
        Constraint::Fill(1),
    ])
    .areas(main);
    let [art_rect] = Layout::vertical([Constraint::Length(art.height)]).flex(Flex::Center).areas(art_col);
    draw_art(f, app, art_rect);
    let (lines, card) = fitted_info(app, p, text_w as usize, main.height, pal, g);
    let info_h = lines.len() as u16 + 2 * u16::from(card);
    let [info] = Layout::vertical([Constraint::Length(info_h)]).flex(Flex::Center).areas(info_col);
    render_info(f, info, lines, card, pal, if card { Alignment::Center } else { Alignment::Left });
}

/// The richest text block that fits in `w` x `h`: a full card, a compact card, compact text,
/// title and progress, or just the title. Returns the lines and whether to frame them.
fn fitted_info(
    app: &App,
    p: &Player,
    w: usize,
    h: u16,
    pal: &Palette,
    g: &Glyphs,
) -> (Vec<Line<'static>>, bool) {
    let h = h as usize;
    for (compact, card) in [(false, true), (true, true), (true, false)] {
        let inner_w = if card { w.saturating_sub(6) } else { w };
        let lines = info_lines(app, p, inner_w, pal, g, compact);
        if lines.len() + 2 * usize::from(card) <= h {
            return (lines, card);
        }
    }
    let mut lines =
        vec![Line::styled(title_text(app, p, w), Style::new().fg(pal.fg).add_modifier(Modifier::BOLD))];
    if h >= 2
        && let Some(progress) = progress_line(app, p, w, pal, g)
    {
        lines.push(progress);
    }
    (lines, false)
}

fn render_info(
    f: &mut Frame,
    area: Rect,
    lines: Vec<Line<'static>>,
    card: bool,
    pal: &Palette,
    align: Alignment,
) {
    let mut paragraph = Paragraph::new(lines).alignment(align);
    if card {
        paragraph = paragraph.block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Style::new().fg(pal.dim))
                .padding(Padding::horizontal(2)),
        );
    }
    f.render_widget(paragraph, area);
}

/// Artwork is scaled up as well as down, so the size setting works for small thumbnails too.
const ART_RESIZE: Resize = Resize::Scale(Some(ratatui_image::FilterType::CatmullRom));

/// Cell size the artwork takes when scaled into `available`, keeping its aspect ratio.
fn art_size(app: &App, available: Size) -> Size {
    if available.width == 0 || available.height == 0 {
        return Size::ZERO;
    }
    app.art.as_ref().and_then(|art| art.size_for(ART_RESIZE, available)).unwrap_or(Size::ZERO)
}

/// Renders the artwork centered in `area`, keeping its aspect ratio.
fn draw_art(f: &mut Frame, app: &mut App, area: Rect) {
    let size = art_size(app, area.as_size());
    let Some(art) = app.art.as_mut() else { return };
    let rect = Rect {
        x: area.x + area.width.saturating_sub(size.width) / 2,
        y: area.y + area.height.saturating_sub(size.height) / 2,
        width: size.width.min(area.width),
        height: size.height.min(area.height),
    };
    f.render_stateful_widget(StatefulImage::default().resize(ART_RESIZE), rect, art);
}

/// "1:23 ━━━━●──── -2:55", shrinking to just the bar on narrow widths. `None` when the player
/// reports no progress at all (common for browser tabs), so nothing is made up.
fn progress_line(app: &App, p: &Player, w: usize, pal: &Palette, g: &Glyphs) -> Option<Line<'static>> {
    let pos = p.position_now();
    match p.length {
        Some(len) => {
            let l = fmt_time(pos);
            let r = if app.cfg.layout.remaining_time {
                format!("-{}", fmt_time(len.saturating_sub(pos)))
            } else {
                fmt_time(len)
            };
            let with_times = w >= l.len() + r.len() + 8;
            let bar_w = if with_times { w - l.len() - r.len() - 2 } else { w };
            let ratio = (pos.as_secs_f64() / len.as_secs_f64().max(0.001)).clamp(0.0, 1.0);
            let done = ((bar_w as f64 * ratio) as usize).min(bar_w.saturating_sub(1));
            let mut spans = vec![
                Span::styled(g.filled.repeat(done), Style::new().fg(pal.accent)),
                Span::styled(g.knob, Style::new().fg(pal.fg)),
                Span::styled(g.empty.repeat(bar_w.saturating_sub(done + 1)), Style::new().fg(pal.dim)),
            ];
            if with_times {
                spans.insert(0, Span::styled(format!("{l} "), Style::new().fg(pal.dim)));
                spans.push(Span::styled(format!(" {r}"), Style::new().fg(pal.dim)));
            }
            Some(Line::from(spans))
        }
        // Length unknown but the player does report position (e.g. a stream): elapsed only.
        None if !p.position.is_zero() => Some(Line::from(vec![
            Span::styled(fmt_time(pos), Style::new().fg(pal.fg)),
            Span::styled(format!(" {} live", g.dot), Style::new().fg(pal.dim)),
        ])),
        None => None,
    }
}

fn controls_line(p: &Player, pal: &Palette, g: &Glyphs) -> Line<'static> {
    let c = p.caps;
    let on = |enabled: bool, s: Style| {
        if enabled && c.control { s } else { Style::new().fg(pal.dim).add_modifier(Modifier::DIM) }
    };
    let toggle = if p.status == Status::Playing { g.pause } else { g.play };
    Line::from(vec![
        Span::styled(g.prev, on(c.prev, Style::new().fg(pal.fg))),
        Span::raw("     "),
        Span::styled(toggle, on(c.play || c.pause, Style::new().fg(pal.accent).add_modifier(Modifier::BOLD))),
        Span::raw("     "),
        Span::styled(g.next, on(c.next, Style::new().fg(pal.fg))),
    ])
}

/// Title, artist, album, progress and controls; `compact` drops the album and spacer rows.
fn info_lines(
    app: &App,
    p: &Player,
    w: usize,
    pal: &Palette,
    g: &Glyphs,
    compact: bool,
) -> Vec<Line<'static>> {
    let layout = &app.cfg.layout;
    let mut lines =
        vec![Line::styled(title_text(app, p, w), Style::new().fg(pal.fg).add_modifier(Modifier::BOLD))];
    if !p.artists.is_empty() {
        lines.push(Line::styled(fit(&p.artists.join(", "), w), Style::new().fg(pal.accent)));
    }
    if !compact && layout.show_album && !p.album.is_empty() {
        lines.push(Line::styled(fit(&p.album, w), Style::new().fg(pal.dim)));
    }
    if let Some(progress) = progress_line(app, p, w, pal, g) {
        if !compact {
            lines.push(Line::raw(""));
        }
        lines.push(progress);
    }
    if layout.show_controls {
        if !compact {
            lines.push(Line::raw(""));
        }
        lines.push(controls_line(p, pal, g));
    }
    lines
}

fn status_label(s: Status) -> &'static str {
    match s {
        Status::Playing => "playing",
        Status::Paused => "paused",
        Status::Stopped => "stopped",
    }
}

/// Status bar; parts are dropped from the end as the width shrinks.
fn draw_footer(f: &mut Frame, app: &App, area: Rect, pal: &Palette, g: &Glyphs) {
    let total = area.width as usize;
    let right = match app.status() {
        Some(s) => format!("{}  ", fit(s, total / 2)),
        None if total >= 60 => "? help  o settings  ".to_string(),
        None => String::new(),
    };
    let budget = total.saturating_sub(right.width());

    let mut left: Vec<Span> = vec![Span::raw(" ")];
    let mut used = 1;
    let mut push = |text: String, style: Style, left: &mut Vec<Span>| {
        if used + text.width() <= budget {
            used += text.width();
            left.push(Span::styled(text, style));
            true
        } else {
            false
        }
    };
    if let Some(p) = app.current() {
        let dot = Style::new().fg(if p.status == Status::Playing { pal.accent } else { pal.dim });
        push(format!("{} ", g.bullet), dot, &mut left);
        // Leading space and bullet take three columns.
        let name = fit(&p.identity, budget.saturating_sub(3).min(24));
        if push(name, Style::new().fg(pal.fg), &mut left) {
            push(format!(" {} {}", g.dot, status_label(p.status)), Style::new().fg(pal.dim), &mut left);
            if app.players.len() > 1 {
                let n = format!("  {} {} players", g.dot, app.players.len());
                push(n, Style::new().fg(pal.dim), &mut left);
            }
        }
    }
    if let Some(reason) = app.viz_fallback.as_deref().filter(|_| app.cfg.visualizer.enabled) {
        push(format!("  {} no audio: {reason}", g.dot), Style::new().fg(pal.dim), &mut left);
    }
    let rw = right.width() as u16;
    let [l, r] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(rw)]).areas(area);
    f.render_widget(Line::from(left), l);
    f.render_widget(
        Span::styled(right, Style::new().fg(if app.status().is_some() { pal.accent } else { pal.dim })),
        r,
    );
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

/// Settings page: tabs on top, the options of the open tab, and a description of the selected
/// option. Borders, spacing and the description give way on small terminals.
fn draw_settings(f: &mut Frame, app: &App, area: Rect, pal: &Palette) {
    let Overlay::Settings { tab, sel, edit } = &app.overlay else { return };
    f.render_widget(Clear, area);
    let roomy = area.width >= 30 && area.height >= 10;
    let inner = if roomy {
        let title = if app.dirty { "Settings  •  unsaved" } else { "Settings" };
        let block = frame_block(title, pal).padding(Padding::horizontal(1));
        let inner = block.inner(area);
        f.render_widget(block, area);
        inner
    } else {
        area
    };
    let w = inner.width as usize;

    let tabs_line = tabs_line(*tab, w, pal);

    let hints = match (edit.is_some(), w >= 70, w >= 40) {
        (true, _, _) => "type · enter apply · esc cancel",
        (false, true, _) => {
            "tab section · ↑↓ select · ←→ change · enter edit · d default · s save · esc close"
        }
        (false, false, true) => "tab · ←→ change · d default · s save · esc",
        _ => "tab ←→ s esc",
    };
    let fields = TABS[*tab].1;
    let field = fields[*sel];
    let help_h = if inner.height >= 9 { 2 } else { 0 };
    let gap = u16::from(inner.height >= 8);
    let hint_h = u16::from(inner.height >= 4);
    // The description sits right under the options; leftover space goes below it.
    let list_h = (fields.len() as u16).min(inner.height.saturating_sub(1 + gap * 2 + help_h + hint_h));
    let [tabs_area, _, list_area, _, help_area, _, hint_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(gap),
        Constraint::Length(list_h),
        Constraint::Length(gap),
        Constraint::Length(help_h),
        Constraint::Fill(1),
        Constraint::Length(hint_h),
    ])
    .areas(inner);
    f.render_widget(tabs_line, tabs_area);

    let lines: Vec<Line> = fields
        .iter()
        .enumerate()
        .map(|(i, &fld)| setting_row(app, fld, i == *sel, edit.as_deref(), w, pal))
        .collect();
    let visible = list_area.height as usize;
    let scroll = (*sel + 1).saturating_sub(visible);
    f.render_widget(Paragraph::new(lines).scroll((scroll as u16, 0)), list_area);

    if help_h > 0 {
        let help = Paragraph::new(settings::help(field))
            .style(Style::new().fg(pal.dim).add_modifier(Modifier::ITALIC))
            .wrap(ratatui::widgets::Wrap { trim: true });
        f.render_widget(help, help_area);
    }
    let hint_text = match app.status() {
        Some(s) => Line::styled(fit(s, w), Style::new().fg(pal.accent)),
        None => Line::styled(fit(hints, w), Style::new().fg(pal.dim)),
    };
    f.render_widget(hint_text, hint_area);
}

/// Tab bar: every name when they fit, otherwise only the open tab with arrows.
fn tabs_line(tab: usize, w: usize, pal: &Palette) -> Line<'static> {
    let names: Vec<String> = TABS.iter().enumerate().map(|(i, (n, _))| format!(" {} {n} ", i + 1)).collect();
    if names.iter().map(|n| n.width()).sum::<usize>() > w {
        let name = format!("‹ {} {}/{} ›", TABS[tab].0, tab + 1, TABS.len());
        return Line::styled(fit(&name, w), Style::new().fg(pal.accent).add_modifier(Modifier::BOLD));
    }
    let open = Style::new().fg(pal.on_accent()).bg(pal.accent).add_modifier(Modifier::BOLD);
    let closed = Style::new().fg(pal.dim);
    names
        .into_iter()
        .enumerate()
        .map(|(i, n)| Span::styled(n, if i == tab { open } else { closed }))
        .collect()
}

/// One option row: label on the left, value (and color swatch) on the right.
fn setting_row(
    app: &App,
    fld: settings::Field,
    selected: bool,
    edit: Option<&str>,
    w: usize,
    pal: &Palette,
) -> Line<'static> {
    let label_w = 20.min(w / 2);
    let value = match edit {
        Some(buf) if selected => format!("{buf}▏"),
        _ if selected && !settings::is_text(fld) => format!("‹ {} ›", settings::value(&app.cfg, fld)),
        _ => settings::value(&app.cfg, fld),
    };
    let value = fit(&value, w.saturating_sub(label_w + 3));
    let swatch = swatch_for(app, fld);
    let base =
        if selected { Style::new().fg(pal.on_accent()).bg(pal.accent) } else { Style::new().fg(pal.fg) };
    let label = fit(settings::label(fld), label_w);
    let pad = w.saturating_sub(1 + label_w + value.width() + 1 + usize::from(swatch.is_some()) * 2);
    let mut spans =
        vec![Span::styled(format!(" {label:<label_w$}"), base), Span::styled(" ".repeat(pad), base)];
    if let Some(c) = swatch {
        spans.push(Span::styled("■ ", base.fg(c)));
    }
    spans.push(Span::styled(format!("{value} "), if selected { base } else { Style::new().fg(pal.accent) }));
    Line::from(spans)
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

    use ratatui::style::Color;
    use unicode_width::UnicodeWidthStr;

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
            page_url: String::new(),
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
        assert!(text(&term).contains("Artist") && text(&term).contains("-2:"));

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

        // No length and no position: the progress row is left out rather than invented.
        player.length = None;
        player.position = Duration::ZERO;
        app.handle(Event::Player(Update::Changed(player)));
        let mut term = Terminal::new(TestBackend::new(100, 40)).unwrap();
        term.draw(|f| super::draw(f, &mut app)).unwrap();
        let screen = text(&term);
        assert!(screen.contains("Artist") && !screen.contains("0:00") && !screen.contains('━'));
    }

    /// App with a playing track, loaded artwork and a full-scale spectrum.
    fn app_with_art() -> (App, tokio::sync::mpsc::UnboundedReceiver<ratatui_image::thread::ResizeRequest>) {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let (rtx, rrx) = tokio::sync::mpsc::unbounded_channel();
        let picker = ratatui_image::picker::Picker::halfblocks();
        let mut cfg = crate::config::Config::default();
        cfg.visualizer.cava_path = "/nonexistent/cava".into();
        cfg.layout.scroll_title = false; // keep the title still; marquee has its own test
        let mut app = App::new(cfg, None, None, tx, picker, rtx);
        app.handle(Event::Player(Update::Changed(Player {
            bus: "b".into(),
            identity: "Mozilla firefox".into(),
            status: Status::Playing,
            title: "ロクデナシ「星を巡る」/ Rokudenashi - Circling the Stars【Official Music Video】".into(),
            artists: vec!["Artist".into()],
            album: "youtube.com".into(),
            art_url: Some("x".into()),
            page_url: String::new(),
            length: Some(Duration::from_secs(200)),
            position: Duration::from_secs(50),
            position_at: Instant::now(),
            rate: 1.0,
            caps: Caps { control: true, play: true, pause: true, next: true, prev: true, seek: true },
        })));
        let img =
            image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(512, 512, image::Rgb([200, 50, 90])));
        app.handle(Event::Art { url: "x".into(), img: Some(img), accent: None });
        app.handle(Event::Spectrum(vec![1.0; 48]));
        (app, rrx)
    }

    /// Draws twice, running the image resize in between as the background worker would.
    fn render(
        app: &mut App,
        rrx: &mut tokio::sync::mpsc::UnboundedReceiver<ratatui_image::thread::ResizeRequest>,
        w: u16,
        h: u16,
    ) -> Terminal<TestBackend> {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| super::draw(f, app)).unwrap();
        while let Ok(req) = rrx.try_recv() {
            app.handle(Event::Resized(req.resize_encode().unwrap()));
        }
        term.draw(|f| super::draw(f, app)).unwrap();
        term
    }

    fn art_cells(term: &Terminal<TestBackend>) -> usize {
        let art = Color::Rgb(200, 50, 90);
        term.backend().buffer().content().iter().filter(|c| c.bg == art || c.fg == art).count()
    }

    /// Artwork and visualizer stay visible at every size, down to tiny terminals.
    #[tokio::test]
    async fn artwork_and_visualizer_at_every_size() {
        let (mut app, mut rrx) = app_with_art();
        for (w, h) in
            [(100, 40), (65, 10), (55, 11), (55, 7), (40, 6), (26, 5), (26, 4), (20, 3), (15, 2), (12, 1)]
        {
            let term = render(&mut app, &mut rrx, w, h);
            let screen = text(&term);
            if std::env::var_os("SHOW").is_some() {
                println!("== {w}x{h}\n{}", term.backend());
            }
            assert!(screen.contains('█'), "no visualizer at {w}x{h}");
            assert!(art_cells(&term) > 0, "no artwork at {w}x{h}");
            if h >= 2 {
                assert!(
                    screen.contains("ロ") || screen.contains("Ro") || screen.contains("Ci"),
                    "no title at {w}x{h}"
                );
            }
        }
        for (w, h) in [(5, 5), (3, 1), (1, 1)] {
            render(&mut app, &mut rrx, w, h); // must not panic
        }
    }

    /// The size setting changes the drawn cover, even for a thumbnail smaller than the screen.
    #[tokio::test]
    async fn artwork_size_setting_scales_small_thumbnails() {
        let (mut app, mut rrx) = app_with_art();
        let thumb =
            image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(64, 36, image::Rgb([200, 50, 90])));
        app.handle(Event::Art { url: "x".into(), img: Some(thumb), accent: None });
        let mut cells = Vec::new();
        for size in [0.2, 0.5, 0.8] {
            app.cfg.artwork.size = size;
            cells.push(art_cells(&render(&mut app, &mut rrx, 120, 50)));
        }
        assert!(cells[0] < cells[1] && cells[1] < cells[2], "artwork did not grow with size: {cells:?}");
    }

    /// The cover gets priority: at least half the screen height on short terminals, ahead of
    /// the visualizer and the text card.
    #[tokio::test]
    async fn cover_gets_priority_on_short_terminals() {
        let (mut app, mut rrx) = app_with_art();
        // Firefox-sized 16:9 thumbnail.
        let thumb =
            image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(336, 188, image::Rgb([200, 50, 90])));
        app.handle(Event::Art { url: "x".into(), img: Some(thumb), accent: None });
        for (w, h) in [(80, 10), (80, 14), (120, 14), (80, 20), (120, 20), (100, 30)] {
            let term = render(&mut app, &mut rrx, w, h);
            let buf = term.backend().buffer();
            let art = Color::Rgb(200, 50, 90);
            let row_has =
                |y: u16, pred: &dyn Fn(&ratatui::buffer::Cell) -> bool| (0..w).any(|x| pred(&buf[(x, y)]));
            let art_rows = (0..h).filter(|&y| row_has(y, &|c| c.bg == art || c.fg == art)).count();
            let viz_rows = (0..h).filter(|&y| row_has(y, &|c| c.symbol() == "█")).count();
            if std::env::var_os("SHOW").is_some() {
                println!("{w}x{h}: cover {art_rows} rows, visualizer {viz_rows} rows");
            }
            assert!(art_rows * 2 >= usize::from(h), "cover too small at {w}x{h}: {art_rows} rows");
            assert!(viz_rows >= 2, "visualizer missing at {w}x{h}");
        }
    }

    /// Sweeps terminal sizes from 1x1 to 240x80: the cover, the visualizer and the title must
    /// all stay visible wherever there is room for them, and nothing may panic.
    #[tokio::test]
    async fn every_terminal_size() {
        let (mut app, mut rrx) = app_with_art();
        let mut failures = Vec::new();
        for w in (1..=240).step_by(9).chain([26, 40, 41, 80, 106, 213]) {
            for h in (1..=80).step_by(4).chain([5, 7, 8, 9, 14, 27, 56]) {
                let term = render(&mut app, &mut rrx, w, h);
                let screen = text(&term);
                if w >= 6 && !screen.contains('█') {
                    failures.push(format!("{w}x{h}: no visualizer"));
                }
                if w >= 10 && art_cells(&term) == 0 {
                    failures.push(format!("{w}x{h}: no artwork"));
                }
                if w >= 6 && h >= 2 && !screen.contains("ロ") {
                    failures.push(format!("{w}x{h}: no title"));
                }
            }
        }
        assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
    }

    /// Out-of-range config values must not crash the renderer.
    #[tokio::test]
    async fn extreme_config_values_render() {
        let (mut app, mut rrx) = app_with_art();
        let v = &mut app.cfg.visualizer;
        (v.spacing, v.bar_width, v.height, v.bars) = (u16::MAX, u16::MAX, u16::MAX, 0);
        v.colors.clear();
        app.cfg.artwork.size = f32::MAX;
        for mode in crate::config::Mode::ALL {
            app.cfg.visualizer.mode = mode;
            for (w, h) in [(120, 40), (26, 5), (1, 1)] {
                render(&mut app, &mut rrx, w, h);
            }
        }
    }

    /// The settings page stays usable on small terminals: tabs, options and navigation.
    #[tokio::test]
    async fn settings_page_on_small_terminals() {
        let (mut app, mut rrx) = app_with_art();
        key(&mut app, KeyCode::Char('o'));
        key(&mut app, KeyCode::Tab); // Layout tab
        let term = render(&mut app, &mut rrx, 26, 5);
        let screen = text(&term);
        assert!(screen.contains("Layout"), "tab name missing:\n{}", term.backend());
        assert!(screen.contains("Status bar"), "options missing:\n{}", term.backend());

        key(&mut app, KeyCode::Right); // toggle status bar off
        assert!(!app.cfg.layout.show_footer);
        key(&mut app, KeyCode::Char('d')); // reset to default
        assert!(app.cfg.layout.show_footer);

        key(&mut app, KeyCode::Char('3')); // Artwork tab by number
        for _ in 0..4 {
            key(&mut app, KeyCode::Down); // Show artwork -> Size -> Shape -> HD -> Position
        }
        let term = render(&mut app, &mut rrx, 80, 24);
        let screen = text(&term);
        assert!(screen.contains("Position") && screen.contains("auto: above"), "{}", term.backend());
        for (w, h) in [(120, 40), (60, 12), (1, 1)] {
            render(&mut app, &mut rrx, w, h);
        }
    }

    #[test]
    fn marquee_scrolls_long_text() {
        assert_eq!(super::marquee("short", 10, 3.0), "short");
        let a = super::marquee("a long title that does not fit", 10, 0.0);
        let b = super::marquee("a long title that does not fit", 10, 1.0);
        assert_eq!(a, "a long tit");
        assert_ne!(a, b);
        assert!(b.width() <= 10);
        assert!(super::marquee("少女レイ少女レイ少女レイ", 7, 0.3).width() <= 7);
    }

    #[test]
    fn fit_counts_wide_chars() {
        assert_eq!(super::fit("abc", 5), "abc");
        assert_eq!(super::fit("abcdef", 4), "abc…");
        assert_eq!(super::fit("少女レイ", 5), "少女…");
    }
}
