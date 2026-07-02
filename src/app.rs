use crate::canvas::{Canvas, Px};
use crate::update::UpdateStatus;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Brush,
    Eraser,
    Fill,
    Eyedropper,
    Select,
}

impl Tool {
    pub fn label(&self) -> &'static str {
        match self {
            Tool::Brush => "Brush",
            Tool::Eraser => "Eraser",
            Tool::Fill => "Fill",
            Tool::Eyedropper => "Pick",
            Tool::Select => "Select",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Tool::Brush => "●",
            Tool::Eraser => "◌",
            Tool::Fill => "▩",
            Tool::Eyedropper => "✚",
            Tool::Select => "▭",
        }
    }

    fn next(&self) -> Tool {
        match self {
            Tool::Brush => Tool::Eraser,
            Tool::Eraser => Tool::Fill,
            Tool::Fill => Tool::Eyedropper,
            Tool::Eyedropper => Tool::Select,
            Tool::Select => Tool::Brush,
        }
    }
}

#[derive(Clone, Copy)]
pub struct SelRect {
    pub x: i32,
    pub y: i32,
    pub w: u16,
    pub h: u16,
}

impl SelRect {
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x
            && px < self.x + self.w as i32
            && py >= self.y
            && py < self.y + self.h as i32
    }
}

pub struct Floating {
    pub x: i32,
    pub y: i32,
    pub w: u16,
    pub h: u16,
    pub cells: Vec<Px>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PickerFocus {
    Sv,
    Hue,
    R,
    G,
    B,
    A,
}

impl PickerFocus {
    fn next(&self) -> PickerFocus {
        match self {
            PickerFocus::Sv => PickerFocus::Hue,
            PickerFocus::Hue => PickerFocus::R,
            PickerFocus::R => PickerFocus::G,
            PickerFocus::G => PickerFocus::B,
            PickerFocus::B => PickerFocus::A,
            PickerFocus::A => PickerFocus::Sv,
        }
    }

    fn prev(&self) -> PickerFocus {
        match self {
            PickerFocus::Sv => PickerFocus::A,
            PickerFocus::Hue => PickerFocus::Sv,
            PickerFocus::R => PickerFocus::Hue,
            PickerFocus::G => PickerFocus::R,
            PickerFocus::B => PickerFocus::G,
            PickerFocus::A => PickerFocus::B,
        }
    }
}

pub struct PickerState {
    pub h: f32,
    pub s: f32,
    pub v: f32,
    pub a: u8,
    pub focus: PickerFocus,
}

impl PickerState {
    pub fn rgb(&self) -> (u8, u8, u8) {
        crate::canvas::hsv_to_rgb(self.h, self.s, self.v)
    }

    pub fn rgba(&self) -> (u8, u8, u8, u8) {
        let (r, g, b) = self.rgb();
        (r, g, b, self.a)
    }

    pub fn set_rgb(&mut self, r: u8, g: u8, b: u8) {
        let (h, s, v) = crate::canvas::rgb_to_hsv(r, g, b);
        self.h = h;
        self.s = s;
        self.v = v;
    }

    pub fn channel(&self, f: PickerFocus) -> u8 {
        let (r, g, b) = self.rgb();
        match f {
            PickerFocus::R => r,
            PickerFocus::G => g,
            PickerFocus::B => b,
            PickerFocus::A => self.a,
            PickerFocus::Sv | PickerFocus::Hue => 0,
        }
    }

    pub fn set_channel(&mut self, f: PickerFocus, val: u8) {
        match f {
            PickerFocus::R => {
                let (_, g, b) = self.rgb();
                self.set_rgb(val, g, b);
            }
            PickerFocus::G => {
                let (r, _, b) = self.rgb();
                self.set_rgb(r, val, b);
            }
            PickerFocus::B => {
                let (r, g, _) = self.rgb();
                self.set_rgb(r, g, val);
            }
            PickerFocus::A => {
                self.a = val;
            }
            PickerFocus::Sv | PickerFocus::Hue => {}
        }
    }
}

pub struct App {
    pub canvas: Canvas,
    pub tool: Tool,
    pub color: u8,          // palette index 0..16
    pub brush_size: u8,     // 1..=4
    pub cursor: (i32, i32), // keyboard cursor in canvas pixel coords
    pub show_help: bool,
    pub status: String, // transient message shown in status bar
    pub quit: bool,
    pub update: UpdateStatus,
    pub update_requested: bool,
    // Hit-test data, written by ui::draw each frame:
    pub canvas_area: Rect,              // inner canvas area (terminal coords)
    pub palette_cells: Vec<(Rect, u8)>, // clickable color swatches
    pub tool_cells: Vec<(Rect, Tool)>,  // clickable tool entries
    pub selection: Option<SelRect>,
    pub floating: Option<Floating>, // only alive during a mouse move-drag
    pub view_x: u16,                // pan offset, canvas pixels
    pub view_y: u16,                // pan offset, canvas pixels; always even
    pub picker: Option<PickerState>,
    pub picker_sv_area: Rect,  // written by ui each frame while the picker is open
    pub picker_hue_area: Rect,
    pub picker_slider_areas: Vec<(Rect, PickerFocus)>, // written by ui each frame while the picker is open
    pub ascii_preview: bool,
    // private:
    drag_last: Option<(i32, i32)>, // last mouse pixel during a stroke
    stroke_active: bool,
    save_counter: u32,
    ascii_counter: u32,
    select_anchor: Option<(i32, i32)>,
    move_grab: Option<(i32, i32)>, // grab offset from floating origin
    clipboard: Option<(u16, u16, Vec<Px>)>,
    pan_last: Option<(u16, u16)>, // terminal cell coords of last middle-drag position
}

impl App {
    pub fn new() -> App {
        App {
            canvas: Canvas::new(0, 0),
            tool: Tool::Brush,
            color: 7,
            brush_size: 1,
            cursor: (0, 0),
            show_help: false,
            status: "press ? for help".to_string(),
            quit: false,
            update: UpdateStatus::Checking,
            update_requested: false,
            canvas_area: Rect::default(),
            palette_cells: Vec::new(),
            tool_cells: Vec::new(),
            selection: None,
            floating: None,
            view_x: 0,
            view_y: 0,
            picker: None,
            picker_sv_area: Rect::default(),
            picker_hue_area: Rect::default(),
            picker_slider_areas: Vec::new(),
            ascii_preview: false,
            drag_last: None,
            stroke_active: false,
            save_counter: 0,
            ascii_counter: 0,
            select_anchor: None,
            move_grab: None,
            clipboard: None,
            pan_last: None,
        }
    }

    /// Stamp any in-progress floating selection back onto the canvas and clear
    /// floating/move_grab state. No-op if there is no floating selection.
    fn commit_floating(&mut self) {
        if let Some(f) = self.floating.take() {
            self.canvas.stamp(f.x, f.y, f.w, f.h, &f.cells);
        }
        self.move_grab = None;
    }

    pub fn clamp_cursor(&mut self) {
        let max_x = self.canvas.width.saturating_sub(1) as i32;
        let max_y = self.canvas.height.saturating_sub(1) as i32;
        if self.canvas.width == 0 {
            self.cursor.0 = 0;
        } else {
            self.cursor.0 = self.cursor.0.clamp(0, max_x);
        }
        if self.canvas.height == 0 {
            self.cursor.1 = 0;
        } else {
            self.cursor.1 = self.cursor.1.clamp(0, max_y);
        }
    }

    fn current_px(&self) -> Px {
        Some(self.color)
    }

    /// Clamp the view offset so the viewport stays inside the canvas; keep view_y even.
    /// Uses self.canvas_area for the viewport size. Safe when canvas_area is 0-sized.
    pub fn clamp_view(&mut self) {
        let vw = self.canvas_area.width;
        let vh = self.canvas_area.height as u16 * 2;
        self.view_x = self.view_x.min(self.canvas.width.saturating_sub(vw));
        self.view_y = self.view_y.min(self.canvas.height.saturating_sub(vh));
        self.view_y &= !1;
    }

    /// Scroll the view minimally so the keyboard cursor's cell is visible.
    fn scroll_cursor_into_view(&mut self) {
        let vw = self.canvas_area.width as i32;
        let vh = self.canvas_area.height as i32 * 2;
        if vw == 0 || vh == 0 {
            return;
        }
        let (cx, cy) = self.cursor;
        let cy_top = cy & !1;
        let mut view_x = self.view_x as i32;
        let mut view_y = self.view_y as i32;
        if cx < view_x {
            view_x = cx;
        }
        if cx >= view_x + vw {
            view_x = cx - vw + 1;
        }
        if cy_top < view_y {
            view_y = cy_top;
        }
        if cy_top + 1 >= view_y + vh {
            view_y = cy_top + 2 - vh;
        }
        self.view_x = view_x.max(0) as u16;
        self.view_y = view_y.max(0) as u16;
        self.clamp_view();
    }

    fn pan_by(&mut self, dx_px: i32, dy_px: i32) {
        let nx = (self.view_x as i32 + dx_px).max(0);
        let ny = (self.view_y as i32 + dy_px).max(0);
        self.view_x = nx as u16;
        self.view_y = ny as u16;
        self.clamp_view();
        self.status = format!("view {},{}", self.view_x, self.view_y);
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.show_help {
            self.show_help = false;
            return;
        }

        if let Some(picker) = &mut self.picker {
            match key.code {
                KeyCode::Esc => {
                    self.picker = None;
                    self.status = "picker cancelled".to_string();
                }
                KeyCode::Enter => {
                    let rgba = picker.rgba();
                    self.canvas.palette[self.color as usize] = rgba;
                    self.picker = None;
                    self.status = format!(
                        "color {} = #{:02X}{:02X}{:02X}{:02X}",
                        self.color, rgba.0, rgba.1, rgba.2, rgba.3
                    );
                }
                KeyCode::Tab => {
                    picker.focus = picker.focus.next();
                }
                KeyCode::BackTab => {
                    picker.focus = picker.focus.prev();
                }
                KeyCode::Char('[') => {
                    picker.h = (picker.h - 4.0).rem_euclid(360.0);
                }
                KeyCode::Char(']') => {
                    picker.h = (picker.h + 4.0).rem_euclid(360.0);
                }
                KeyCode::Left => match picker.focus {
                    PickerFocus::Sv => picker.s = (picker.s - 0.02).clamp(0.0, 1.0),
                    PickerFocus::Hue => picker.h = (picker.h - 4.0).rem_euclid(360.0),
                    PickerFocus::R | PickerFocus::G | PickerFocus::B | PickerFocus::A => {
                        let f = picker.focus;
                        let val = picker.channel(f).saturating_sub(1);
                        picker.set_channel(f, val);
                    }
                },
                KeyCode::Right => match picker.focus {
                    PickerFocus::Sv => picker.s = (picker.s + 0.02).clamp(0.0, 1.0),
                    PickerFocus::Hue => picker.h = (picker.h + 4.0).rem_euclid(360.0),
                    PickerFocus::R | PickerFocus::G | PickerFocus::B | PickerFocus::A => {
                        let f = picker.focus;
                        let val = picker.channel(f).saturating_add(1);
                        picker.set_channel(f, val);
                    }
                },
                KeyCode::Up => match picker.focus {
                    PickerFocus::Sv => picker.v = (picker.v + 0.02).clamp(0.0, 1.0),
                    PickerFocus::Hue => {}
                    PickerFocus::R | PickerFocus::G | PickerFocus::B | PickerFocus::A => {
                        let f = picker.focus;
                        let val = picker.channel(f).saturating_add(10);
                        picker.set_channel(f, val);
                    }
                },
                KeyCode::Down => match picker.focus {
                    PickerFocus::Sv => picker.v = (picker.v - 0.02).clamp(0.0, 1.0),
                    PickerFocus::Hue => {}
                    PickerFocus::R | PickerFocus::G | PickerFocus::B | PickerFocus::A => {
                        let f = picker.focus;
                        let val = picker.channel(f).saturating_sub(10);
                        picker.set_channel(f, val);
                    }
                },
                _ => {}
            }
            return;
        }

        if self.ascii_preview {
            match key.code {
                KeyCode::Esc | KeyCode::Char('a') => {
                    self.ascii_preview = false;
                    self.status = "ascii preview closed".to_string();
                }
                KeyCode::Enter => {
                    self.save_ascii();
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.quit = true;
                return;
            }
            KeyCode::Char('c') => {
                let (r, g, b, a) = self.canvas.palette[self.color as usize];
                let (h, s, v) = crate::canvas::rgb_to_hsv(r, g, b);
                self.picker = Some(PickerState {
                    h,
                    s,
                    v,
                    a,
                    focus: PickerFocus::Sv,
                });
                self.status = format!("editing color {} — Enter apply, Esc cancel", self.color);
                return;
            }
            KeyCode::Char('a') => {
                self.ascii_preview = true;
                self.status = "ascii preview — Enter save, Esc close".to_string();
                return;
            }
            KeyCode::Char('q') => {
                self.quit = true;
                return;
            }
            KeyCode::Esc => {
                if self.floating.is_some() {
                    self.commit_floating();
                    self.status = "moved".to_string();
                } else if self.selection.is_some() || self.select_anchor.is_some() {
                    self.selection = None;
                    self.select_anchor = None;
                    self.status = "deselected".to_string();
                } else {
                    self.quit = true;
                }
                return;
            }
            KeyCode::Char('?') => {
                self.show_help = true;
                return;
            }
            _ => {}
        }

        match key.code {
            KeyCode::Char('b') => {
                self.commit_floating();
                self.tool = Tool::Brush;
                self.status = "tool: brush".to_string();
            }
            KeyCode::Char('e') => {
                self.commit_floating();
                self.tool = Tool::Eraser;
                self.status = "tool: eraser".to_string();
            }
            KeyCode::Char('f') => {
                self.commit_floating();
                self.tool = Tool::Fill;
                self.status = "tool: fill".to_string();
            }
            KeyCode::Char('i') => {
                self.commit_floating();
                self.tool = Tool::Eyedropper;
                self.status = "tool: eyedropper".to_string();
            }
            KeyCode::Char('m') => {
                self.tool = Tool::Select;
                self.status = "tool: select".to_string();
            }
            KeyCode::Tab => {
                self.commit_floating();
                self.tool = self.tool.next();
                self.status = format!("tool: {}", self.tool.label().to_lowercase());
            }
            KeyCode::Char(c @ '1'..='8') => {
                let idx = c as u8 - b'1';
                self.color = idx;
                self.status = format!("color {}", self.color);
            }
            KeyCode::Char('[') => {
                self.color = (self.color + 15) % 16;
                self.status = format!("color {}", self.color);
            }
            KeyCode::Char(']') => {
                self.color = (self.color + 1) % 16;
                self.status = format!("color {}", self.color);
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                if self.brush_size < 4 {
                    self.brush_size += 1;
                }
                self.status = format!("size {}", self.brush_size);
            }
            KeyCode::Char('-') => {
                if self.brush_size > 1 {
                    self.brush_size -= 1;
                }
                self.status = format!("size {}", self.brush_size);
            }
            KeyCode::Char('u') => {
                if self.canvas.undo() {
                    self.status = "undo".to_string();
                } else {
                    self.status = "nothing to undo".to_string();
                }
            }
            KeyCode::Char('U') => {
                if matches!(self.update, UpdateStatus::Available { .. }) {
                    self.update_requested = true;
                } else {
                    self.status = "no update available".to_string();
                }
            }
            KeyCode::Char('r') => {
                if self.canvas.redo() {
                    self.status = "redo".to_string();
                } else {
                    self.status = "nothing to redo".to_string();
                }
            }
            KeyCode::Char('x') => {
                self.canvas.snapshot();
                self.canvas.clear();
                self.status = "canvas cleared".to_string();
            }
            KeyCode::Char('s') => {
                self.save_png();
            }
            KeyCode::Char('y') => {
                if let Some(sel) = self.selection {
                    let cells = self.canvas.copy_rect(sel.x, sel.y, sel.w, sel.h);
                    self.clipboard = Some((sel.w, sel.h, cells));
                    self.status = "copied".to_string();
                }
            }
            KeyCode::Char('p') => {
                if let Some((w, h, cells)) = self.clipboard.clone() {
                    self.canvas.snapshot();
                    let (x, y) = self.cursor;
                    self.canvas.stamp(x, y, w, h, &cells);
                    self.selection = Some(SelRect { x, y, w, h });
                    self.tool = Tool::Select;
                    self.status = "pasted".to_string();
                }
            }
            KeyCode::Char('d') | KeyCode::Delete | KeyCode::Backspace => {
                if let Some(sel) = self.selection {
                    self.canvas.snapshot();
                    self.canvas.clear_rect(sel.x, sel.y, sel.w, sel.h);
                    self.status = "deleted".to_string();
                }
            }
            KeyCode::Up => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.pan_by(0, -4);
                    return;
                }
                self.move_cursor_or_selection(0, -1)
            }
            KeyCode::Down => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.pan_by(0, 4);
                    return;
                }
                self.move_cursor_or_selection(0, 1)
            }
            KeyCode::Left => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.pan_by(-4, 0);
                    return;
                }
                self.move_cursor_or_selection(-1, 0)
            }
            KeyCode::Right => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.pan_by(4, 0);
                    return;
                }
                self.move_cursor_or_selection(1, 0)
            }
            KeyCode::Char(' ') => {
                self.apply_tool_at_cursor();
            }
            _ => {}
        }
    }

    /// Arrow-key handler: when the Select tool is active with a live (non-floating)
    /// selection, nudges the selected pixels 1px; otherwise moves the keyboard cursor.
    fn move_cursor_or_selection(&mut self, dx: i32, dy: i32) {
        if self.tool == Tool::Select && self.floating.is_none() {
            if let Some(mut sel) = self.selection {
                self.canvas.snapshot();
                let cells = self.canvas.copy_rect(sel.x, sel.y, sel.w, sel.h);
                self.canvas.clear_rect(sel.x, sel.y, sel.w, sel.h);
                sel.x += dx;
                sel.y += dy;
                self.canvas.stamp(sel.x, sel.y, sel.w, sel.h, &cells);
                self.selection = Some(sel);
                self.cursor.0 += dx;
                self.cursor.1 += dy;
                self.clamp_cursor();
                self.scroll_cursor_into_view();
                return;
            }
        }
        self.cursor.0 += dx;
        self.cursor.1 += dy;
        self.clamp_cursor();
        self.scroll_cursor_into_view();
    }

    fn apply_tool_at_cursor(&mut self) {
        let (x, y) = self.cursor;
        match self.tool {
            Tool::Brush => {
                self.canvas.snapshot();
                let px = self.current_px();
                self.canvas.paint(x, y, self.brush_size, px);
                self.status = "painted".to_string();
            }
            Tool::Eraser => {
                self.canvas.snapshot();
                self.canvas.paint(x, y, self.brush_size, None);
                self.status = "erased".to_string();
            }
            Tool::Fill => {
                self.canvas.snapshot();
                let px = self.current_px();
                self.canvas.flood_fill(x, y, px);
                self.status = "filled".to_string();
            }
            Tool::Eyedropper => {
                if let Some(c) = self.canvas.get(x, y) {
                    self.color = c;
                    self.status = format!("picked color {}", c);
                }
            }
            Tool::Select => {
                self.status = "drag on canvas to select".to_string();
            }
        }
    }

    fn save_png(&mut self) {
        if let Err(e) = std::fs::create_dir_all("paintings") {
            self.status = format!("save failed: {}", e);
            return;
        }
        loop {
            let name = format!("paintings/painting-{:02}.png", self.save_counter);
            let path = std::path::Path::new(&name);
            if !path.exists() {
                match self.canvas.export_png(path) {
                    Ok(()) => {
                        self.status = format!("saved {}", name);
                    }
                    Err(e) => {
                        self.status = format!("save failed: {}", e);
                    }
                }
                self.save_counter += 1;
                break;
            }
            self.save_counter += 1;
        }
    }

    fn save_ascii(&mut self) {
        if let Err(e) = std::fs::create_dir_all("paintings") {
            self.status = format!("save failed: {}", e);
            return;
        }
        loop {
            let name = format!("paintings/ascii-{:02}.txt", self.ascii_counter);
            let path = std::path::Path::new(&name);
            if !path.exists() {
                let contents = self.canvas.to_ascii().join("\n") + "\n";
                match std::fs::write(path, contents) {
                    Ok(()) => {
                        self.status = format!("saved {}", name);
                        self.ascii_preview = false;
                    }
                    Err(e) => {
                        self.status = format!("save failed: {}", e);
                    }
                }
                self.ascii_counter += 1;
                break;
            }
            self.ascii_counter += 1;
        }
    }

    fn mouse_to_canvas_px(&self, m: &MouseEvent) -> Option<(i32, i32)> {
        let area = self.canvas_area;
        if m.column >= area.x
            && m.column < area.x + area.width
            && m.row >= area.y
            && m.row < area.y + area.height
        {
            let x = (m.column - area.x) as i32 + self.view_x as i32;
            let y = ((m.row - area.y) as i32) * 2 + self.view_y as i32;
            Some((x, y))
        } else {
            None
        }
    }

    fn hit_test_palette(&self, m: &MouseEvent) -> Option<u8> {
        for (rect, idx) in &self.palette_cells {
            if m.column >= rect.x
                && m.column < rect.x + rect.width
                && m.row >= rect.y
                && m.row < rect.y + rect.height
            {
                return Some(*idx);
            }
        }
        None
    }

    fn hit_test_tool(&self, m: &MouseEvent) -> Option<Tool> {
        for (rect, tool) in &self.tool_cells {
            if m.column >= rect.x
                && m.column < rect.x + rect.width
                && m.row >= rect.y
                && m.row < rect.y + rect.height
            {
                return Some(*tool);
            }
        }
        None
    }

    /// Shared drag-painting logic used by both mouse buttons for non-Select tools.
    /// `erase` forces None regardless of the active tool/color (used for right-drag).
    fn paint_drag(&mut self, m: &MouseEvent, erase: bool) {
        if self.stroke_active {
            if let Some((x, y)) = self.mouse_to_canvas_px(m) {
                self.cursor = (x, y);
                if let Some((lx, ly)) = self.drag_last {
                    let px = if erase { None } else { self.current_px() };
                    self.canvas.line(lx, ly, x, y, self.brush_size, px);
                    self.canvas.line(lx, ly + 1, x, y + 1, self.brush_size, px);
                }
                self.drag_last = Some((x, y));
            }
        }
    }

    pub fn handle_mouse(&mut self, m: MouseEvent) {
        if self.show_help {
            if matches!(m.kind, MouseEventKind::Down(_)) {
                self.show_help = false;
            }
            return;
        }

        if let Some(picker) = &mut self.picker {
            if matches!(
                m.kind,
                MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left)
            ) {
                let sv = self.picker_sv_area;
                let hue = self.picker_hue_area;
                if m.column >= sv.x
                    && m.column < sv.x + sv.width
                    && m.row >= sv.y
                    && m.row < sv.y + sv.height
                    && sv.width >= 1
                    && sv.height >= 1
                {
                    let col = m.column;
                    let row = m.row;
                    picker.focus = PickerFocus::Sv;
                    picker.s = ((col - sv.x) as f32 / (sv.width.saturating_sub(1).max(1)) as f32)
                        .clamp(0.0, 1.0);
                    picker.v = (1.0
                        - ((row - sv.y) as f32 + 0.5) / sv.height as f32)
                        .clamp(0.0, 1.0);
                } else if m.column >= hue.x
                    && m.column < hue.x + hue.width
                    && m.row >= hue.y
                    && m.row < hue.y + hue.height
                    && hue.width >= 1
                    && hue.height >= 1
                {
                    let col = m.column;
                    picker.focus = PickerFocus::Hue;
                    picker.h = (col - hue.x) as f32
                        / (hue.width.saturating_sub(1).max(1)) as f32
                        * 359.9;
                } else {
                    for (rect, focus) in self.picker_slider_areas.clone() {
                        if m.column >= rect.x
                            && m.column < rect.x + rect.width
                            && m.row >= rect.y
                            && m.row < rect.y + rect.height
                            && rect.width >= 1
                        {
                            let col = m.column;
                            let val = ((col - rect.x) as f32
                                / (rect.width.saturating_sub(1).max(1)) as f32
                                * 255.0)
                                .round()
                                .clamp(0.0, 255.0) as u8;
                            picker.focus = focus;
                            picker.set_channel(focus, val);
                            break;
                        }
                    }
                }
            }
            return;
        }

        if self.ascii_preview {
            if matches!(m.kind, MouseEventKind::Down(_)) {
                self.ascii_preview = false;
                self.status = "ascii preview closed".to_string();
            }
            return;
        }

        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(idx) = self.hit_test_palette(&m) {
                    self.color = idx;
                    self.status = format!("color {}", self.color);
                    return;
                }
                if let Some(tool) = self.hit_test_tool(&m) {
                    self.commit_floating();
                    self.tool = tool;
                    self.status = format!("tool: {}", self.tool.label().to_lowercase());
                    return;
                }
                if let Some((x, y)) = self.mouse_to_canvas_px(&m) {
                    self.cursor = (x, y);
                    if self.tool == Tool::Select {
                        let hit = self
                            .selection
                            .map_or(false, |s| s.contains(x, y) || s.contains(x, y + 1));
                        if hit {
                            let sel = self.selection.unwrap();
                            self.canvas.snapshot();
                            let cells = self.canvas.copy_rect(sel.x, sel.y, sel.w, sel.h);
                            self.canvas.clear_rect(sel.x, sel.y, sel.w, sel.h);
                            self.floating = Some(Floating {
                                x: sel.x,
                                y: sel.y,
                                w: sel.w,
                                h: sel.h,
                                cells,
                            });
                            self.move_grab = Some((x - sel.x, y - sel.y));
                            self.status = "moving".to_string();
                        } else {
                            self.select_anchor = Some((x, y));
                            self.selection = Some(SelRect { x, y, w: 1, h: 2 });
                            self.status = "selecting".to_string();
                        }
                    } else {
                        match self.tool {
                            Tool::Brush => {
                                self.canvas.snapshot();
                                let px = self.current_px();
                                self.canvas.paint(x, y, self.brush_size, px);
                                self.canvas.paint(x, y + 1, self.brush_size, px);
                                self.stroke_active = true;
                                self.drag_last = Some((x, y));
                            }
                            Tool::Eraser => {
                                self.canvas.snapshot();
                                self.canvas.paint(x, y, self.brush_size, None);
                                self.canvas.paint(x, y + 1, self.brush_size, None);
                                self.stroke_active = true;
                                self.drag_last = Some((x, y));
                            }
                            Tool::Fill => {
                                self.canvas.snapshot();
                                let px = self.current_px();
                                self.canvas.flood_fill(x, y, px);
                                self.status = "filled".to_string();
                            }
                            Tool::Eyedropper => {
                                if let Some(c) = self.canvas.get(x, y) {
                                    self.color = c;
                                    self.status = format!("picked color {}", c);
                                }
                            }
                            Tool::Select => unreachable!(),
                        }
                    }
                }
            }
            MouseEventKind::Down(MouseButton::Middle) => {
                self.pan_last = Some((m.column, m.row));
                self.status = "panning".to_string();
            }
            MouseEventKind::Down(MouseButton::Right) => {
                if let Some((x, y)) = self.mouse_to_canvas_px(&m) {
                    self.cursor = (x, y);
                    self.canvas.snapshot();
                    self.canvas.paint(x, y, self.brush_size, None);
                    self.canvas.paint(x, y + 1, self.brush_size, None);
                    self.stroke_active = true;
                    self.drag_last = Some((x, y));
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.tool == Tool::Select {
                    if let Some((x, y)) = self.mouse_to_canvas_px(&m) {
                        if let Some(grab) = self.move_grab {
                            if let Some(f) = &mut self.floating {
                                let nx = x - grab.0;
                                let ny = y - grab.1;
                                f.x = nx;
                                f.y = ny;
                                self.selection = Some(SelRect {
                                    x: nx,
                                    y: ny,
                                    w: f.w,
                                    h: f.h,
                                });
                            }
                            self.cursor = (x, y);
                        } else if let Some((ax, ay)) = self.select_anchor {
                            let x0 = ax.min(x);
                            let x1 = ax.max(x);
                            let y0 = ay.min(y);
                            let y1 = ay.max(y) + 1;
                            self.selection = Some(SelRect {
                                x: x0,
                                y: y0,
                                w: (x1 - x0 + 1) as u16,
                                h: (y1 - y0 + 1) as u16,
                            });
                            self.cursor = (x, y);
                        }
                    }
                } else {
                    self.paint_drag(&m, self.tool == Tool::Eraser);
                }
            }
            MouseEventKind::Drag(MouseButton::Right) => {
                self.paint_drag(&m, true);
            }
            MouseEventKind::Drag(MouseButton::Middle) => {
                if let Some((lx, ly)) = self.pan_last {
                    self.pan_by(
                        lx as i32 - m.column as i32,
                        (ly as i32 - m.row as i32) * 2,
                    );
                    self.pan_last = Some((m.column, m.row));
                }
            }
            MouseEventKind::Up(_) => {
                if self.floating.is_some() {
                    self.commit_floating();
                    self.status = "moved".to_string();
                }
                self.select_anchor = None;
                self.stroke_active = false;
                self.drag_last = None;
                self.pan_last = None;
            }
            MouseEventKind::ScrollUp => {
                if self.brush_size < 4 {
                    self.brush_size += 1;
                }
                self.status = format!("size {}", self.brush_size);
            }
            MouseEventKind::ScrollDown => {
                if self.brush_size > 1 {
                    self.brush_size -= 1;
                }
                self.status = format!("size {}", self.brush_size);
            }
            _ => {}
        }
    }
}
