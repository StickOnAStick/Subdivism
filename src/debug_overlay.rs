use bytemuck::{Pod, Zeroable};

const OVERLAY_MARGIN: f32 = 16.0;
const OVERLAY_LINE_HEIGHT: f32 = 28.0;
const OVERLAY_GLYPH_SCALE: f32 = 3.0;
const MENU_PANEL_MIN_WIDTH: f32 = 480.0;
const MENU_PANEL_HORIZONTAL_PADDING: f32 = 40.0;
const MENU_PANEL_TOP_PADDING: f32 = 24.0;
const MENU_PANEL_BOTTOM_PADDING: f32 = 20.0;
const MENU_ENTRY_HEIGHT: f32 = 32.0;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct OverlayVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

impl OverlayVertex {
    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<OverlayVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

pub struct DebugOverlay {
    pub visible: bool,
    fps: f32,
    frame_count: u32,
    elapsed: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn contains(&self, point_x: f32, point_y: f32) -> bool {
        point_x >= self.x
            && point_x <= self.x + self.width
            && point_y >= self.y
            && point_y <= self.y + self.height
    }
}

#[derive(Clone, Debug)]
pub struct MenuLayout {
    pub panel: Rect,
    pub item_rects: Vec<Rect>,
}

impl DebugOverlay {
    pub fn new() -> Self {
        Self {
            visible: false,
            fps: 0.0,
            frame_count: 0,
            elapsed: 0.0,
        }
    }

    pub fn record_frame(&mut self, dt: f32) {
        self.frame_count += 1;
        self.elapsed += dt;

        if self.elapsed >= 0.25 {
            self.fps = self.frame_count as f32 / self.elapsed.max(f32::EPSILON);
            self.frame_count = 0;
            self.elapsed = 0.0;
        }
    }

    pub fn current_fps(&self) -> f32 {
        self.fps
    }

    pub fn build_vertices(&self, memory_bytes: u64, extra_lines: &[String]) -> Vec<OverlayVertex> {
        if !self.visible {
            return Vec::new();
        }

        let fps_line = format!("FPS {:.1}", self.fps);
        let memory_line = format!("GPU MEM {:.2} MB", memory_bytes as f64 / (1024.0 * 1024.0));
        let mut lines = vec![fps_line, memory_line];
        lines.extend(extra_lines.iter().cloned());

        let glyph_h = 7.0 * OVERLAY_GLYPH_SCALE;
        let advance = 6.0 * OVERLAY_GLYPH_SCALE;
        let panel_width = lines
            .iter()
            .map(|line| line.chars().count() as f32 * advance)
            .fold(0.0, f32::max)
            + 20.0;
        let panel_height = OVERLAY_MARGIN + lines.len() as f32 * OVERLAY_LINE_HEIGHT + glyph_h
            - OVERLAY_GLYPH_SCALE;

        let mut vertices = Vec::new();
        push_overlay_quad(
            &mut vertices,
            OVERLAY_MARGIN - 8.0,
            OVERLAY_MARGIN - 8.0,
            panel_width,
            panel_height,
            [0.02, 0.03, 0.05, 0.78],
        );
        push_overlay_quad(
            &mut vertices,
            OVERLAY_MARGIN - 4.0,
            OVERLAY_MARGIN - 4.0,
            panel_width,
            panel_height,
            [0.12, 0.16, 0.22, 0.35],
        );

        for (line_index, line) in lines.iter().enumerate() {
            let y = OVERLAY_MARGIN + line_index as f32 * OVERLAY_LINE_HEIGHT;
            push_text(
                &mut vertices,
                OVERLAY_MARGIN + 1.0,
                y + 1.0,
                line,
                OVERLAY_GLYPH_SCALE,
                [0.10, 0.16, 0.14, 0.85],
            );
            push_text(
                &mut vertices,
                OVERLAY_MARGIN,
                y,
                line,
                OVERLAY_GLYPH_SCALE,
                [0.82, 0.96, 0.88, 1.0],
            );
        }

        vertices
    }

    pub fn build_menu_vertices(
        &self,
        title: &str,
        lines: &[String],
        selected_index: usize,
        hovered_index: Option<usize>,
        screen_size: [f32; 2],
    ) -> Vec<OverlayVertex> {
        let mut vertices = Vec::new();
        let layout = self.menu_layout(title, lines, screen_size);
        let panel = layout.panel;
        let glyph_h = 7.0 * OVERLAY_GLYPH_SCALE;
        let advance = 6.0 * OVERLAY_GLYPH_SCALE;

        push_overlay_quad(
            &mut vertices,
            panel.x,
            panel.y,
            panel.width,
            panel.height,
            [0.04, 0.06, 0.07, 0.90],
        );
        push_overlay_quad(
            &mut vertices,
            panel.x + 4.0,
            panel.y + 4.0,
            panel.width - 8.0,
            panel.height - 8.0,
            [0.10, 0.15, 0.12, 0.54],
        );

        let title_width = title.chars().count() as f32 * advance;
        let title_x = panel.x + (panel.width - title_width) * 0.5;
        let title_y = panel.y + MENU_PANEL_TOP_PADDING;
        push_text(
            &mut vertices,
            title_x + 1.0,
            title_y + 1.0,
            title,
            OVERLAY_GLYPH_SCALE,
            [0.06, 0.08, 0.09, 0.9],
        );
        push_text(
            &mut vertices,
            title_x,
            title_y,
            title,
            OVERLAY_GLYPH_SCALE,
            [0.94, 0.86, 0.62, 1.0],
        );

        for (line_index, line) in lines.iter().enumerate() {
            let item_rect = layout.item_rects[line_index];
            let is_selected = line_index == selected_index;
            let is_hovered = hovered_index == Some(line_index);
            if is_selected || is_hovered {
                let highlight = if is_selected {
                    [0.28, 0.46, 0.44, 0.58]
                } else {
                    [0.18, 0.30, 0.28, 0.45]
                };
                push_overlay_quad(
                    &mut vertices,
                    item_rect.x,
                    item_rect.y,
                    item_rect.width,
                    item_rect.height,
                    highlight,
                );
            }

            let y = item_rect.y + (item_rect.height - glyph_h) * 0.5;
            let line_width = line.chars().count() as f32 * advance;
            let x = panel.x + (panel.width - line_width) * 0.5;
            let foreground = if is_selected {
                [0.84, 0.96, 0.92, 1.0]
            } else if is_hovered {
                [0.78, 0.90, 0.86, 1.0]
            } else {
                [0.88, 0.92, 0.91, 1.0]
            };
            push_text(
                &mut vertices,
                x + 1.0,
                y + 1.0,
                line,
                OVERLAY_GLYPH_SCALE,
                [0.03, 0.05, 0.05, 0.95],
            );
            push_text(&mut vertices, x, y, line, OVERLAY_GLYPH_SCALE, foreground);
        }

        vertices
    }

    pub fn menu_layout(&self, title: &str, lines: &[String], screen_size: [f32; 2]) -> MenuLayout {
        let advance = 6.0 * OVERLAY_GLYPH_SCALE;
        let widest_text = std::iter::once(title)
            .chain(lines.iter().map(String::as_str))
            .map(|line| line.chars().count() as f32 * advance)
            .fold(0.0, f32::max);
        let panel_width = (widest_text + MENU_PANEL_HORIZONTAL_PADDING * 2.0)
            .max(MENU_PANEL_MIN_WIDTH)
            .min(screen_size[0] - OVERLAY_MARGIN * 2.0);
        let panel_height = MENU_PANEL_TOP_PADDING
            + OVERLAY_LINE_HEIGHT
            + lines.len() as f32 * MENU_ENTRY_HEIGHT
            + MENU_PANEL_BOTTOM_PADDING;
        let panel_x = ((screen_size[0] - panel_width) * 0.5).max(OVERLAY_MARGIN);
        let panel_y = ((screen_size[1] - panel_height) * 0.5).max(OVERLAY_MARGIN);

        let mut item_rects = Vec::with_capacity(lines.len());
        for index in 0..lines.len() {
            let y = panel_y
                + MENU_PANEL_TOP_PADDING
                + OVERLAY_LINE_HEIGHT
                + index as f32 * MENU_ENTRY_HEIGHT;
            item_rects.push(Rect {
                x: panel_x + 10.0,
                y,
                width: panel_width - 20.0,
                height: MENU_ENTRY_HEIGHT - 2.0,
            });
        }

        MenuLayout {
            panel: Rect {
                x: panel_x,
                y: panel_y,
                width: panel_width,
                height: panel_height,
            },
            item_rects,
        }
    }

    pub fn add_quad(
        vertices: &mut Vec<OverlayVertex>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [f32; 4],
    ) {
        push_overlay_quad(vertices, x, y, width, height, color);
    }

    pub fn add_text(
        vertices: &mut Vec<OverlayVertex>,
        x: f32,
        y: f32,
        text: &str,
        scale: f32,
        color: [f32; 4],
    ) {
        push_text(vertices, x, y, text, scale, color);
    }
}

fn push_overlay_quad(
    vertices: &mut Vec<OverlayVertex>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: [f32; 4],
) {
    let x2 = x + width;
    let y2 = y + height;
    vertices.extend_from_slice(&[
        OverlayVertex {
            position: [x, y],
            color,
        },
        OverlayVertex {
            position: [x2, y],
            color,
        },
        OverlayVertex {
            position: [x2, y2],
            color,
        },
        OverlayVertex {
            position: [x, y],
            color,
        },
        OverlayVertex {
            position: [x2, y2],
            color,
        },
        OverlayVertex {
            position: [x, y2],
            color,
        },
    ]);
}

fn push_text(
    vertices: &mut Vec<OverlayVertex>,
    x: f32,
    y: f32,
    text: &str,
    scale: f32,
    color: [f32; 4],
) {
    let mut cursor_x = x;
    for ch in text.chars() {
        let glyph = glyph_rows(ch);
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..5 {
                if (bits >> (4 - col)) & 1 == 1 {
                    push_overlay_quad(
                        vertices,
                        cursor_x + col as f32 * scale,
                        y + row as f32 * scale,
                        scale,
                        scale,
                        color,
                    );
                }
            }
        }
        cursor_x += 6.0 * scale;
    }
}

fn glyph_rows(ch: char) -> [u8; 7] {
    match ch {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
        ],
        'D' => [
            0b11100, 0b10010, 0b10001, 0b10001, 0b10001, 0b10010, 0b11100,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        'J' => [
            0b00001, 0b00001, 0b00001, 0b00001, 0b10001, 0b10001, 0b01110,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10001, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
        ],
        'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
        ],
        '6' => [
            0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b11100,
        ],
        '.' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00110, 0b00110,
        ],
        '-' => [
            0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000,
        ],
        '_' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b11111,
        ],
        ' ' => [0; 7],
        _ => [0; 7],
    }
}
