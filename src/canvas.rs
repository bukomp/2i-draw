use std::collections::VecDeque;
use std::path::Path;

use anyhow::Result;
use image::{Rgba, RgbaImage};

/// A single canvas pixel: `Some(i)` indexes into [`PALETTE`], `None` is empty/transparent.
pub type Px = Option<u8>;

/// 16-color, pico-8-inspired palette. Index 0..16.
pub const PALETTE: [(u8, u8, u8, u8); 16] = [
    (0, 0, 0, 255),       // 0  black
    (29, 43, 83, 255),    // 1  dark blue
    (126, 37, 83, 255),   // 2  dark purple
    (0, 135, 81, 255),    // 3  dark green
    (171, 82, 54, 255),   // 4  brown
    (95, 87, 79, 255),    // 5  dark grey
    (194, 195, 199, 255), // 6  light grey
    (255, 241, 232, 255), // 7  white
    (255, 0, 77, 255),    // 8  red
    (255, 163, 0, 255),   // 9  orange
    (255, 236, 39, 255),  // 10 yellow
    (0, 228, 54, 255),    // 11 green
    (41, 173, 255, 255),  // 12 blue
    (131, 118, 156, 255), // 13 lavender
    (255, 119, 168, 255), // 14 pink
    (255, 204, 170, 255), // 15 peach
];

const UNDO_CAP: usize = 100;

/// A stored canvas state used for undo/redo.
struct Snap {
    width: u16,
    height: u16,
    cells: Vec<Px>,
}

pub struct Canvas {
    pub width: u16,  // pixels
    pub height: u16, // pixels
    cells: Vec<Px>,  // row-major, len = width*height
    undo_stack: Vec<Snap>,
    redo_stack: Vec<Snap>,
    /// Runtime, editable palette (RGBA, indexed by the same u8 indices stored in cells).
    /// Initialized to [`PALETTE`]; not touched by resize/undo/redo/snapshot.
    pub palette: [(u8, u8, u8, u8); 16],
}

impl Canvas {
    pub fn new(width: u16, height: u16) -> Self {
        let len = width as usize * height as usize;
        Self {
            width,
            height,
            cells: vec![None; len],
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            palette: PALETTE,
        }
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 {
            return None;
        }
        let (x, y) = (x as u32, y as u32);
        if x >= self.width as u32 || y >= self.height as u32 {
            return None;
        }
        Some(y as usize * self.width as usize + x as usize)
    }

    pub fn get(&self, x: i32, y: i32) -> Px {
        self.index(x, y).and_then(|i| self.cells[i])
    }

    pub fn set(&mut self, x: i32, y: i32, px: Px) {
        if let Some(i) = self.index(x, y) {
            self.cells[i] = px;
        }
    }

    /// Square brush of side `size` (1..=4) centered on (x,y) (for even sizes bias top-left).
    pub fn paint(&mut self, x: i32, y: i32, size: u8, px: Px) {
        let size = size.clamp(1, 4) as i32;
        // For even sizes, bias top-left: offsets run from -(size/2) .. size/2 - 1 (adjusted below).
        let before = (size - 1) / 2;
        let after = size / 2;
        for dy in -before..=after {
            for dx in -before..=after {
                self.set(x + dx, y + dy, px);
            }
        }
    }

    /// Bresenham line from (x0,y0) to (x1,y1), calling paint at each point.
    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, size: u8, px: Px) {
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let (mut x, mut y) = (x0, y0);

        loop {
            self.paint(x, y, size, px);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Iterative flood fill (BFS) of the region whose current value equals get(x,y),
    /// replacing with px. No-op if target == px or OOB.
    pub fn flood_fill(&mut self, x: i32, y: i32, px: Px) {
        let target = match self.index(x, y) {
            Some(_) => self.get(x, y),
            None => return,
        };
        if target == px {
            return;
        }

        let mut queue: VecDeque<(i32, i32)> = VecDeque::new();
        queue.push_back((x, y));

        while let Some((cx, cy)) = queue.pop_front() {
            // must bounds-check via index: get() reads OOB as None, which would
            // match a None target and expand the BFS outside the canvas forever
            let Some(i) = self.index(cx, cy) else { continue };
            if self.cells[i] != target {
                continue;
            }
            self.cells[i] = px;
            queue.push_back((cx + 1, cy));
            queue.push_back((cx - 1, cy));
            queue.push_back((cx, cy + 1));
            queue.push_back((cx, cy - 1));
        }
    }

    /// All None.
    pub fn clear(&mut self) {
        for c in self.cells.iter_mut() {
            *c = None;
        }
    }

    /// Resize preserving existing content top-left anchored; does NOT touch undo stacks.
    pub fn resize(&mut self, width: u16, height: u16) {
        let mut new_cells = vec![None; width as usize * height as usize];
        let copy_w = self.width.min(width) as usize;
        let copy_h = self.height.min(height) as usize;
        for y in 0..copy_h {
            for x in 0..copy_w {
                let src = y * self.width as usize + x;
                let dst = y * width as usize + x;
                new_cells[dst] = self.cells[src];
            }
        }
        self.width = width;
        self.height = height;
        self.cells = new_cells;
    }

    /// Push current cells onto undo stack (cap 100, drop oldest), clear redo stack.
    /// Call BEFORE a mutating operation begins.
    pub fn snapshot(&mut self) {
        self.undo_stack.push(Snap {
            width: self.width,
            height: self.height,
            cells: self.cells.clone(),
        });
        if self.undo_stack.len() > UNDO_CAP {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    /// Restore from undo stack (current goes to redo); false if empty. If the stored
    /// snapshot has different dimensions than current, restore then re-resize to current dims.
    pub fn undo(&mut self) -> bool {
        let Some(snap) = self.undo_stack.pop() else {
            return false;
        };
        let cur_w = self.width;
        let cur_h = self.height;
        self.redo_stack.push(Snap {
            width: self.width,
            height: self.height,
            cells: self.cells.clone(),
        });
        self.width = snap.width;
        self.height = snap.height;
        self.cells = snap.cells;
        if self.width != cur_w || self.height != cur_h {
            self.resize(cur_w, cur_h);
        }
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(snap) = self.redo_stack.pop() else {
            return false;
        };
        let cur_w = self.width;
        let cur_h = self.height;
        self.undo_stack.push(Snap {
            width: self.width,
            height: self.height,
            cells: self.cells.clone(),
        });
        self.width = snap.width;
        self.height = snap.height;
        self.cells = snap.cells;
        if self.width != cur_w || self.height != cur_h {
            self.resize(cur_w, cur_h);
        }
        true
    }

    /// Read a w×h region row-major, top-left (x,y); out-of-bounds pixels read as None. No mutation.
    pub fn copy_rect(&self, x: i32, y: i32, w: u16, h: u16) -> Vec<Px> {
        let mut out = Vec::with_capacity(w as usize * h as usize);
        for dy in 0..h as i32 {
            for dx in 0..w as i32 {
                out.push(self.get(x + dx, y + dy));
            }
        }
        out
    }

    /// Set every pixel in the region to None (OOB parts ignored).
    pub fn clear_rect(&mut self, x: i32, y: i32, w: u16, h: u16) {
        for dy in 0..h as i32 {
            for dx in 0..w as i32 {
                self.set(x + dx, y + dy, None);
            }
        }
    }

    /// Write a row-major w×h buffer at (x,y). None entries are SKIPPED (transparent),
    /// OOB writes ignored. cells.len() must be treated as w*h (use .get() defensively).
    pub fn stamp(&mut self, x: i32, y: i32, w: u16, h: u16, cells: &[Px]) {
        for dy in 0..h as i32 {
            for dx in 0..w as i32 {
                let idx = dy as usize * w as usize + dx as usize;
                if let Some(Some(px)) = cells.get(idx) {
                    self.set(x + dx, y + dy, Some(*px));
                }
            }
        }
    }

    /// Export as PNG scaled x8 per pixel (square: each canvas pixel -> 8x8 px block).
    /// None pixels -> opaque dark background rgb(24,24,32).
    pub fn export_png(&self, path: &Path) -> Result<()> {
        const SCALE: u32 = 8;
        const BG: Rgba<u8> = Rgba([24, 24, 32, 255]);

        let img_w = self.width as u32 * SCALE;
        let img_h = self.height as u32 * SCALE;
        let mut img = RgbaImage::from_pixel(img_w.max(1), img_h.max(1), BG);

        for y in 0..self.height {
            for x in 0..self.width {
                let px = self.get(x as i32, y as i32);
                let color = match px {
                    Some(i) => {
                        let (r, g, b, a) = self.palette[i as usize % self.palette.len()];
                        Rgba([r, g, b, a])
                    }
                    None => BG,
                };
                let base_x = x as u32 * SCALE;
                let base_y = y as u32 * SCALE;
                for dy in 0..SCALE {
                    for dx in 0..SCALE {
                        img.put_pixel(base_x + dx, base_y + dy, color);
                    }
                }
            }
        }

        img.save(path)?;
        Ok(())
    }

    /// Render the canvas as ASCII art: one output char per column × 2-pixel row pair
    /// (matches the on-screen half-block geometry). Luminance ramp " .:-=+*#%@";
    /// lum(px) = (0.2126 r + 0.7152 g + 0.0722 b) / 255 * (a / 255), None = 0.
    /// Char = RAMP[round(avg_lum_of_pair * 9)]; if either pixel of the pair is painted
    /// but the index lands on 0, use '.' instead of ' ' (dark colors must stay visible).
    /// Returns height.div_ceil(2) lines of exactly `width` chars.
    pub fn to_ascii(&self) -> Vec<String> {
        const RAMP: &[u8] = b" .:-=+*#%@";

        let lum = |px: Px| -> f32 {
            match px {
                Some(i) => {
                    let (r, g, b, a) = self.palette[i as usize % self.palette.len()];
                    let l = (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0;
                    l * (a as f32 / 255.0)
                }
                None => 0.0,
            }
        };

        let out_h = (self.height as usize).div_ceil(2);
        let mut lines = Vec::with_capacity(out_h);

        for oy in 0..out_h {
            let y0 = oy as i32 * 2;
            let y1 = y0 + 1;
            let mut line = String::with_capacity(self.width as usize);
            for x in 0..self.width as i32 {
                let px0 = self.get(x, y0);
                let px1 = self.get(x, y1);
                let painted = px0.is_some() || px1.is_some();
                let avg = (lum(px0) + lum(px1)) / 2.0;
                let idx = (avg * 9.0).round().clamp(0.0, 9.0) as usize;
                let ch = if painted && idx == 0 {
                    '.'
                } else {
                    RAMP[idx] as char
                };
                line.push(ch);
            }
            lines.push(line);
        }

        lines
    }
}

/// h in [0,360), s and v in [0,1] → rgb.
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let h = h.rem_euclid(360.0);
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);

    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r1, g1, b1) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    let r = ((r1 + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    let g = ((g1 + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    let b = ((b1 + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (r, g, b)
}

/// rgb → (h in [0,360), s, v in [0,1]).
pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;

    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let delta = max - min;

    let h = if delta.abs() < 1e-6 {
        0.0
    } else if max == rf {
        60.0 * (((gf - bf) / delta).rem_euclid(6.0))
    } else if max == gf {
        60.0 * (((bf - rf) / delta) + 2.0)
    } else {
        60.0 * (((rf - gf) / delta) + 4.0)
    };
    let h = h.rem_euclid(360.0);

    let s = if max.abs() < 1e-6 { 0.0 } else { delta / max };
    let v = max;

    (h, s.clamp(0.0, 1.0), v.clamp(0.0, 1.0))
}
