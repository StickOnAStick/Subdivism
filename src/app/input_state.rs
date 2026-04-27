use std::collections::HashSet;

use winit::keyboard::KeyCode;

use super::InventorySection;

#[derive(Clone, Copy)]
pub(super) struct InventoryCursor {
    pub(super) section: InventorySection,
    pub(super) index: usize,
}

pub(super) struct InputState {
    pub(super) pressed: HashSet<KeyCode>,
    pub(super) mouse_captured: bool,
    pub(super) cursor_position: Option<(f32, f32)>,
    look_delta_pixels: (f32, f32),
}

impl InputState {
    pub(super) fn new() -> Self {
        Self {
            pressed: HashSet::new(),
            mouse_captured: false,
            cursor_position: None,
            look_delta_pixels: (0.0, 0.0),
        }
    }

    pub(super) fn key(&self, key: KeyCode) -> bool {
        self.pressed.contains(&key)
    }

    pub(super) fn accumulate_look_delta(&mut self, delta_x: f32, delta_y: f32) {
        self.look_delta_pixels.0 += delta_x;
        self.look_delta_pixels.1 += delta_y;
    }

    pub(super) fn consume_look_delta(&mut self) -> (f32, f32) {
        let look_delta = self.look_delta_pixels;
        self.look_delta_pixels = (0.0, 0.0);
        look_delta
    }

    pub(super) fn clear_look_delta(&mut self) {
        self.look_delta_pixels = (0.0, 0.0);
    }
}
