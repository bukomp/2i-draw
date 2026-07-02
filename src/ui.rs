use crate::app::{App, Tool};
use crate::canvas::hsv_to_rgb;
use crate::update::UpdateStatus;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

const ACCENT: Color = Color::Rgb(120, 110, 200);
const DIM_BG_A: Color = Color::Rgb(20, 20, 28);
const DIM_BG_B: Color = Color::Rgb(26, 26, 34);
const STATUS_BG: Color = Color::Rgb(30, 28, 44);
const SELECT_FG: Color = Color::Rgb(255, 220, 120);
const SEL_TINT: (u8, u8, u8) = (150, 140, 235);
const SEL_EMPTY_A: Color = Color::Rgb(45, 45, 70);
const SEL_EMPTY_B: Color = Color::Rgb(52, 52, 78);

const TOOLS: [Tool; 5] = [
    Tool::Brush,
    Tool::Eraser,
    Tool::Fill,
    Tool::Eyedropper,
    Tool::Select,
];

fn tool_key(tool: Tool) -> &'static str {
    match tool {
        Tool::Brush => "b",
        Tool::Eraser => "e",
        Tool::Fill => "f",
        Tool::Eyedropper => "i",
        Tool::Select => "m",
    }
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let root = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(18), Constraint::Min(1)])
        .split(frame.area());
    let sidebar_area = root[0];
    let main_area = root[1];

    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(main_area);
    let canvas_outer = main[0];
    let status_area = main[1];

    draw_sidebar(frame, app, sidebar_area);
    draw_canvas(frame, app, canvas_outer);
    draw_status(frame, app, status_area);

    if app.show_help {
        draw_help(frame, frame.area());
    }

    draw_picker(frame, app);
}

fn draw_sidebar(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .title(" ✦ idraw ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let buf = frame.buffer_mut();
    let mut y = inner.y;

    // tools header
    buf.set_string(
        inner.x,
        y,
        "tools",
        Style::default().add_modifier(Modifier::DIM | Modifier::BOLD),
    );
    y += 1;

    app.tool_cells.clear();
    for tool in TOOLS {
        if y >= inner.y + inner.height {
            break;
        }
        let selected = app.tool == tool;
        let text = format!("{} {}  {}", tool.icon(), tool.label(), tool_key(tool));
        let style = if selected {
            Style::default().fg(SELECT_FG).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let row_rect = Rect::new(inner.x, y, inner.width, 1);
        buf.set_string(inner.x, y, &text, style);
        app.tool_cells.push((row_rect, tool));
        y += 1;
    }

    y += 1; // blank line

    if y < inner.y + inner.height {
        buf.set_string(
            inner.x,
            y,
            "palette",
            Style::default().add_modifier(Modifier::DIM | Modifier::BOLD),
        );
        y += 1;
    }

    app.palette_cells.clear();
    const SWATCH_W: u16 = 4; // marker(1) + 3 colored spaces
    const GAP: u16 = 1;
    for row in 0..8u8 {
        if y >= inner.y + inner.height {
            break;
        }
        for col in 0..2u8 {
            let idx = row * 2 + col;
            if idx as usize >= app.canvas.palette.len() {
                continue;
            }
            let x = inner.x + col as u16 * (SWATCH_W + GAP);
            if x + SWATCH_W > inner.x + inner.width {
                continue;
            }
            let selected = app.color == idx;
            let marker = if selected { '▸' } else { ' ' };
            let (r, g, b) = app.canvas.palette[idx as usize];
            buf.set_string(x, y, marker.to_string(), Style::default());
            let swatch_rect = Rect::new(x, y, SWATCH_W, 1);
            buf.set_string(
                x + 1,
                y,
                "   ",
                Style::default().bg(Color::Rgb(r, g, b)),
            );
            app.palette_cells.push((swatch_rect, idx));
        }
        y += 1;
    }

    y += 1; // blank line

    if y < inner.y + inner.height {
        let filled = app.brush_size.min(4);
        let mut dots = String::from("size ");
        for i in 0..4u8 {
            dots.push(if i < filled { '●' } else { '○' });
        }
        buf.set_string(inner.x, y, &dots, Style::default());
        y += 1;
    }
    let _ = y;

    // bottom hint
    if inner.height >= 1 {
        let hint_y = inner.y + inner.height - 1;
        if matches!(app.update, UpdateStatus::Available { .. }) && hint_y > inner.y {
            let update_y = hint_y - 1;
            buf.set_string(
                inner.x,
                update_y,
                "⬆ update — U",
                Style::default().fg(SELECT_FG).add_modifier(Modifier::BOLD),
            );
        }
        buf.set_string(
            inner.x,
            hint_y,
            "? help  q quit",
            Style::default().add_modifier(Modifier::DIM),
        );
    }
}

fn draw_canvas(frame: &mut Frame, app: &mut App, area: Rect) {
    // Probe the inner rect (border geometry) before we know the final title text.
    let probe_inner = Block::default().borders(Borders::ALL).inner(area);
    app.canvas_area = probe_inner;

    if probe_inner.width != 0 && probe_inner.height != 0 {
        // Grow-only resize: never shrink the canvas, even if the viewport shrinks.
        let nw = app.canvas.width.max(probe_inner.width);
        let nh = app.canvas.height.max(probe_inner.height * 2);
        if (nw, nh) != (app.canvas.width, app.canvas.height) {
            app.canvas.resize(nw, nh);
        }
    }
    app.clamp_view();
    app.clamp_cursor();

    let vw = probe_inner.width;
    let vh = probe_inner.height * 2;
    let title = if app.view_x != 0 || app.view_y != 0 || app.canvas.width > vw || app.canvas.height > vh
    {
        format!(
            " canvas {}×{} @ {},{} ",
            app.canvas.width, app.canvas.height, app.view_x, app.view_y
        )
    } else {
        format!(" canvas {}×{} ", app.canvas.width, app.canvas.height)
    };
    let block = Block::default()
        .title(title)
        .title_style(Style::default().add_modifier(Modifier::DIM))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let buf = frame.buffer_mut();
    let view_x = app.view_x as i32;
    let view_y = app.view_y as i32;
    for row in 0..inner.height {
        for col in 0..inner.width {
            let ax = view_x + col as i32;
            let top_ay = view_y + row as i32 * 2;
            let bottom_ay = top_ay + 1;
            let top = effective_px(app, ax, top_ay);
            let bottom = effective_px(app, ax, bottom_ay);
            let top_in_sel = app
                .selection
                .map_or(false, |sel| sel.contains(ax, top_ay));
            let bottom_in_sel = app
                .selection
                .map_or(false, |sel| sel.contains(ax, bottom_ay));

            let fg = pixel_color(top, ax, top_ay, top_in_sel, &app.canvas.palette);
            let bg = pixel_color(bottom, ax, bottom_ay, bottom_in_sel, &app.canvas.palette);

            let cx = inner.x + col;
            let cy = inner.y + row;
            buf.set_string(cx, cy, "▀", Style::default().fg(fg).bg(bg));
        }
    }

    // keyboard cursor overlay
    let (cx, cy) = app.cursor;
    if cx >= 0 && cy >= 0 {
        let rel_x = cx - view_x;
        let rel_y = cy - view_y;
        if rel_x >= 0 && rel_y >= 0 {
            let rel_row = rel_y / 2;
            if (rel_x as u16) < inner.width && (rel_row as u16) < inner.height {
                let term_x = inner.x + rel_x as u16;
                let term_y = inner.y + rel_row as u16;
                if let Some(cell) = buf.cell_mut((term_x, term_y)) {
                    cell.set_style(Style::default().add_modifier(Modifier::REVERSED));
                }
            }
        }
    }
}

/// Resolve the pixel that should actually be drawn at (x,y), accounting for
/// a floating layer (which wins over the canvas when it has an opaque cell).
fn effective_px(app: &App, x: i32, y: i32) -> Option<u8> {
    if let Some(floating) = &app.floating {
        if x >= floating.x
            && x < floating.x + floating.w as i32
            && y >= floating.y
            && y < floating.y + floating.h as i32
        {
            let col = (x - floating.x) as usize;
            let row = (y - floating.y) as usize;
            let idx = row * floating.w as usize + col;
            if let Some(Some(c)) = floating.cells.get(idx) {
                return Some(*c);
            }
        }
    }
    app.canvas.get(x, y)
}

fn pixel_color(
    px: Option<u8>,
    x: i32,
    y: i32,
    in_sel: bool,
    palette: &[(u8, u8, u8); 16],
) -> Color {
    match px {
        Some(i) => {
            let (r, g, b) = palette[i as usize % palette.len()];
            if in_sel {
                let (ar, ag, ab) = SEL_TINT;
                let blend = |c: u8, a: u8| -> u8 {
                    let c = c as f32;
                    let a = a as f32;
                    (c + 0.35 * (a - c)).round().clamp(0.0, 255.0) as u8
                };
                Color::Rgb(blend(r, ar), blend(g, ag), blend(b, ab))
            } else {
                Color::Rgb(r, g, b)
            }
        }
        None => {
            if in_sel {
                if (x + y) % 2 == 0 {
                    SEL_EMPTY_A
                } else {
                    SEL_EMPTY_B
                }
            } else if (x + y) % 2 == 0 {
                DIM_BG_A
            } else {
                DIM_BG_B
            }
        }
    }
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    if area.height == 0 {
        return;
    }
    let (r, g, b) = app.canvas.palette[app.color as usize % app.canvas.palette.len()];
    let left = format!(
        "{} {} │ size {} │ color ",
        app.tool.icon(),
        app.tool.label(),
        app.brush_size
    );
    let right = app.status.clone();

    let left_len = left.chars().count() as u16 + 2; // + swatch width "██"
    let right_len = right.chars().count() as u16;
    let width = area.width;
    let pad = width.saturating_sub(left_len + right_len);

    let line = Line::from(vec![
        Span::raw(left),
        Span::styled("██", Style::default().fg(Color::Rgb(r, g, b))),
        Span::raw(" ".repeat(pad as usize)),
        Span::styled(right, Style::default().add_modifier(Modifier::DIM)),
    ]);

    let para = Paragraph::new(line).style(Style::default().bg(STATUS_BG));
    frame.render_widget(para, area);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let popup = centered_rect(48, 28, area);
    frame.render_widget(Clear, popup);

    let lines = vec![
        Line::from("Mouse"),
        Line::from("  left click/drag   paint"),
        Line::from("  right click/drag  erase"),
        Line::from("  wheel             brush size"),
        Line::from("  drag (select)      marquee select"),
        Line::from("  drag inside sel    move pixels"),
        Line::from("  middle drag        pan view"),
        Line::from(""),
        Line::from("Keys"),
        Line::from("  b/e/f/i   brush/eraser/fill/pick"),
        Line::from("  m         select tool"),
        Line::from("  Tab       cycle tool"),
        Line::from("  1-8       color  [ ]  cycle color"),
        Line::from("  c         color picker (edit slot)"),
        Line::from("  + / -     brush size"),
        Line::from("  arrows    move cursor"),
        Line::from("  Shift+arrows  pan view"),
        Line::from("  space     apply tool at cursor"),
        Line::from("  y / p     copy / paste"),
        Line::from("  d / Del   delete selection"),
        Line::from("  u / r     undo / redo"),
        Line::from("  x         clear canvas"),
        Line::from("  s         save PNG"),
        Line::from("  U         self-update from git"),
        Line::from("  ?         toggle this help"),
        Line::from("  q         quit"),
        Line::from("  Esc       deselect / quit"),
    ];

    let block = Block::default()
        .title(" keys ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT));

    let para = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Left);
    frame.render_widget(para, popup);
}

fn draw_picker(frame: &mut Frame, app: &mut App) {
    let Some(picker) = &app.picker else {
        return;
    };
    let (h, s, v) = (picker.h, picker.s, picker.v);
    let (r, g, b) = hsv_to_rgb(h, s, v);

    let popup = centered_rect(44, 15, frame.area());
    frame.render_widget(Clear, popup);

    let title = format!(" color {} ", app.color);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.width == 0 || inner.height == 0 {
        app.picker_sv_area = Rect::default();
        app.picker_hue_area = Rect::default();
        return;
    }

    let sv_h = inner.height.saturating_sub(4).max(1).min(inner.height);
    let sv_area = Rect::new(inner.x, inner.y, inner.width, sv_h);
    let hue_y = (inner.y + sv_h + 1).min(inner.y + inner.height - 1);
    let hue_area = Rect::new(inner.x, hue_y, inner.width, 1);
    let readout_y = inner.y + inner.height - 1;

    app.picker_sv_area = sv_area;
    app.picker_hue_area = hue_area;

    let buf = frame.buffer_mut();

    // SV square
    if sv_area.width >= 1 && sv_area.height >= 1 {
        let w = sv_area.width;
        let rows = sv_area.height;
        // nearest cell to current (s, v)
        let cur_col = ((s * (w.saturating_sub(1).max(1)) as f32).round() as u16).min(w.saturating_sub(1));
        let cur_row = if rows >= 1 {
            let total_steps = (rows as u32 * 2).saturating_sub(1).max(1) as f32;
            // v = 1 - (row*2 + 0.5)/total_steps  =>  row = ((1-v)*total_steps - 0.5) / 2
            let approx = (((1.0 - v) * total_steps - 0.5) / 2.0).round();
            approx.clamp(0.0, (rows.saturating_sub(1)) as f32) as u16
        } else {
            0
        };

        for row in 0..rows {
            for col in 0..w {
                let cs = col as f32 / (w.saturating_sub(1).max(1)) as f32;
                let total_steps = (rows as u32 * 2).saturating_sub(1).max(1) as f32;
                let v_top = 1.0 - ((row as f32 * 2.0 + 0.0) / total_steps);
                let v_bottom = 1.0 - ((row as f32 * 2.0 + 1.0) / total_steps);
                let (fr, fg_, fb) = hsv_to_rgb(h, cs, v_top.clamp(0.0, 1.0));
                let (br, bg_, bb) = hsv_to_rgb(h, cs, v_bottom.clamp(0.0, 1.0));

                let cx = sv_area.x + col;
                let cy = sv_area.y + row;
                let mut style = Style::default()
                    .fg(Color::Rgb(fr, fg_, fb))
                    .bg(Color::Rgb(br, bg_, bb));
                if col == cur_col && row == cur_row {
                    style = style.add_modifier(Modifier::REVERSED);
                }
                buf.set_string(cx, cy, "▀", style);
            }
        }
    }

    // Hue bar
    if hue_area.width >= 1 {
        let w = hue_area.width;
        let cur_hue_col = ((h / 359.9) * (w.saturating_sub(1).max(1)) as f32)
            .round()
            .clamp(0.0, (w.saturating_sub(1)) as f32) as u16;
        for col in 0..w {
            let hue = col as f32 / (w.saturating_sub(1).max(1)) as f32 * 359.9;
            let (hr, hg, hb) = hsv_to_rgb(hue, 1.0, 1.0);
            let mut style = Style::default().fg(Color::Rgb(hr, hg, hb));
            if col == cur_hue_col {
                style = style.add_modifier(Modifier::REVERSED);
            }
            buf.set_string(hue_area.x + col, hue_area.y, "█", style);
        }
    }

    // Readout
    if inner.width >= 1 {
        let hex = format!("#{:02X}{:02X}{:02X}", r, g, b);
        let text = format!(
            "  ██ {}   ←→ s · ↑↓ v · [ ] h · Enter ✓ · Esc ✗",
            hex
        );
        let truncated: String = text.chars().take(inner.width as usize).collect();
        // Render base text dim, then overlay the swatch chars with the color style.
        buf.set_string(
            inner.x,
            readout_y,
            &truncated,
            Style::default().add_modifier(Modifier::DIM),
        );
        // Overlay the "██" swatch (positioned right after the 2 leading spaces).
        if truncated.chars().count() >= 4 {
            buf.set_string(
                inner.x + 2,
                readout_y,
                "██",
                Style::default().fg(Color::Rgb(r, g, b)),
            );
        }
    }
}
