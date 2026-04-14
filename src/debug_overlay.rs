use bytemuck::{Pod, Zeroable};

const OVERLAY_MARGIN: f32 = 16.0;
const OVERLAY_LINE_HEIGHT: f32 = 28.0;
const OVERLAY_GLYPH_SCALE: f32 = 3.0;

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

    pub fn build_vertices(&self, memory_bytes: u64) -> Vec<OverlayVertex> {
        if !self.visible {
            return Vec::new();
        }

        let fps_line = format!("FPS {:.1}", self.fps);
        let memory_line = format!("GPU MEM {:.2} MB", memory_bytes as f64 / (1024.0 * 1024.0));
        let lines = [fps_line, memory_line];

        let glyph_h = 7.0 * OVERLAY_GLYPH_SCALE;
        let advance = 6.0 * OVERLAY_GLYPH_SCALE;
        let panel_width = lines
            .iter()
            .map(|line| line.chars().count() as f32 * advance)
            .fold(0.0, f32::max)
            + 20.0;
        let panel_height =
            OVERLAY_MARGIN + lines.len() as f32 * OVERLAY_LINE_HEIGHT + glyph_h - OVERLAY_GLYPH_SCALE;

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
    ) -> Vec<OverlayVertex> {
        let mut vertices = Vec::new();
        let mut all_lines = Vec::with_capacity(lines.len() + 1);
        all_lines.push(title.to_string());
        all_lines.extend(lines.iter().cloned());

        let glyph_h = 7.0 * OVERLAY_GLYPH_SCALE;
        let advance = 6.0 * OVERLAY_GLYPH_SCALE;
        let panel_width = all_lines
            .iter()
            .map(|line| line.chars().count() as f32 * advance)
            .fold(0.0, f32::max)
            + 32.0;
        let panel_height =
            20.0 + all_lines.len() as f32 * OVERLAY_LINE_HEIGHT + glyph_h - OVERLAY_GLYPH_SCALE;
        let panel_x = OVERLAY_MARGIN;
        let panel_y = 120.0;

        push_overlay_quad(
            &mut vertices,
            panel_x,
            panel_y,
            panel_width,
            panel_height,
            [0.02, 0.03, 0.05, 0.88],
        );
        push_overlay_quad(
            &mut vertices,
            panel_x + 3.0,
            panel_y + 3.0,
            panel_width - 6.0,
            panel_height - 6.0,
            [0.10, 0.13, 0.18, 0.60],
        );

        for (line_index, line) in all_lines.iter().enumerate() {
            let y = panel_y + 16.0 + line_index as f32 * OVERLAY_LINE_HEIGHT;
            let line_width = line.chars().count() as f32 * advance;
            let x = panel_x + (panel_width - line_width) * 0.5;
            let is_title = line_index == 0;
            let is_selected = !is_title && (line_index - 1) == selected_index;
            let foreground = if is_title {
                [0.92, 0.82, 0.45, 1.0]
            } else if is_selected {
                [0.74, 0.90, 0.98, 1.0]
            } else {
                [0.91, 0.94, 0.98, 1.0]
            };
            push_text(
                &mut vertices,
                x + 1.0,
                y + 1.0,
                line,
                OVERLAY_GLYPH_SCALE,
                [0.05, 0.08, 0.10, 0.95],
            );
            push_text(
                &mut vertices,
                x,
                y,
                line,
                OVERLAY_GLYPH_SCALE,
                foreground,
            );
        }

        vertices
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
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'D' => [0b11100, 0b10010, 0b10001, 0b10001, 0b10001, 0b10010, 0b11100],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'J' => [0b00001, 0b00001, 0b00001, 0b00001, 0b10001, 0b10001, 0b01110],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10001, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'Q' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010],
        'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
        'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b11100],
        '.' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00110, 0b00110],
        ' ' => [0; 7],
        _ => [0; 7],
    }
}
