use std::path::PathBuf;

use winit::keyboard::KeyCode;

use super::{
    App, MAX_TERRAIN_LAB_SAVE_NAME_CHARS, TERRAIN_PRESET_RECIPE_DIR, TerrainConfig,
    TerrainParamRegistry, TerrainRecipe, UiMode, slider_f32,
};

impl App {
    pub(super) fn terrain_lab_overlay(&self) -> Option<(String, Vec<String>, usize)> {
        if !self.session.terrain_lab.enabled || !self.session.terrain_lab.panel_visible {
            return None;
        }
        let keys = TerrainConfig::parameter_keys();
        let max_rows = 12_usize;
        let selected = self
            .session
            .terrain_lab
            .selected_index
            .min(keys.len().saturating_sub(1));
        let page_start = selected.saturating_sub(max_rows / 2);
        let page_end = (page_start + max_rows).min(keys.len());
        let shown = &keys[page_start..page_end];
        let mut lines = Vec::with_capacity(shown.len() + 8);
        lines.push("WHEEL/UPDOWN SELECT LEFTRIGHT TUNE".to_string());
        lines.push("SHIFT FINE CTRL ULTRA-FINE PGUP/PGDN COARSE".to_string());
        lines.push("F8 PANEL F9 SAVE PRESET F10 EDIT NAME".to_string());
        if self.session.terrain_lab.editing_save_name {
            lines.push("NAME EDIT ACTIVE TYPE / BKSP ENTER=SAVE ESC=CANCEL".to_string());
        } else {
            lines.push("NAME EDIT OFF".to_string());
        }
        lines.push(format!("SAVE NAME {}", self.session.terrain_lab.save_name));
        if !self.session.terrain_lab.last_save_status.is_empty() {
            lines.push(self.session.terrain_lab.last_save_status.clone());
        }
        let header_rows = lines.len();
        let cfg = self.world.terrain_config();
        for key in shown {
            let value = TerrainParamRegistry::value(&cfg, key);
            lines.push(format!(
                "{} {}",
                TerrainParamRegistry::label(key),
                slider_f32(
                    value,
                    TerrainParamRegistry::min(key),
                    TerrainParamRegistry::max(key),
                    12
                )
            ));
        }
        Some((
            "TERRAIN LAB".to_string(),
            lines,
            (selected - page_start) + header_rows,
        ))
    }

    fn terrain_lab_step_for(&self, key: &str) -> f32 {
        let base = TerrainParamRegistry::step(key);
        let ctrl_held =
            self.input.key(KeyCode::ControlLeft) || self.input.key(KeyCode::ControlRight);
        let shift_held = self.input.key(KeyCode::ShiftLeft) || self.input.key(KeyCode::ShiftRight);
        if ctrl_held {
            base * 0.1
        } else if shift_held {
            base * 0.25
        } else {
            base
        }
    }

    fn terrain_lab_adjust_selected(&mut self, delta: f32) {
        if !self.session.terrain_lab.enabled {
            return;
        }
        let keys = TerrainConfig::parameter_keys();
        if keys.is_empty() {
            return;
        }
        let index = self.session.terrain_lab.selected_index.min(keys.len() - 1);
        let key = keys[index];
        let mut cfg = self.world.terrain_config();
        let current = TerrainParamRegistry::value(&cfg, key);
        if cfg.set_named_param(key, current + delta).is_err() {
            return;
        }
        self.rebuild_world_from_terrain(cfg, true);
    }

    fn terrain_lab_save_preset(&mut self) {
        if !self.session.terrain_lab.enabled {
            return;
        }
        let typed_name = self.session.terrain_lab.save_name.trim();
        let terrain_name = if typed_name.is_empty() {
            default_terrain_lab_save_name(self.session.world_seed)
        } else {
            typed_name.to_string()
        };
        self.session.terrain_lab.save_name = terrain_name.clone();
        let file_stem = sanitize_terrain_recipe_name(&terrain_name);
        let named_path =
            PathBuf::from(TERRAIN_PRESET_RECIPE_DIR).join(format!("{file_stem}.terrain"));
        let default_path = PathBuf::from(super::DEFAULT_TERRAIN_RECIPE_PATH);

        let mut recipe = TerrainRecipe::balanced(self.session.world_seed);
        recipe.name = terrain_name;
        recipe.description = "Saved from in-game Terrain Lab".to_string();
        recipe.profile = "terrain_lab".to_string();
        recipe.seed = self.session.world_seed;
        recipe.terrain = self.world.terrain_config();
        if let Err(err) = recipe.write_to_file(&named_path) {
            self.session.terrain_lab.last_save_status = format!("SAVE FAILED {err}");
            eprintln!(
                "failed to save named terrain preset {}: {err}",
                named_path.display()
            );
            return;
        }
        if let Err(err) = recipe.write_to_file(&default_path) {
            self.session.terrain_lab.last_save_status = format!("DEFAULT IMPORT FAILED {err}");
            eprintln!(
                "failed to update default terrain recipe {}: {err}",
                default_path.display()
            );
            return;
        }
        self.session.terrain_profile = "terrain_lab".to_string();
        self.session.terrain_recipe_path = Some(default_path.clone());
        self.session.terrain_lab.last_save_status = format!(
            "SAVED {} (DEFAULT IMPORT {})",
            named_path.display(),
            default_path.display()
        );
        eprintln!(
            "terrain lab preset saved to {} and imported as {}",
            named_path.display(),
            default_path.display()
        );
    }

    fn handle_terrain_lab_name_input(&mut self, code: KeyCode) -> bool {
        if !self.session.terrain_lab.editing_save_name {
            return false;
        }
        match code {
            KeyCode::Escape => {
                self.session.terrain_lab.editing_save_name = false;
                return true;
            }
            KeyCode::Enter => {
                self.terrain_lab_save_preset();
                self.session.terrain_lab.editing_save_name = false;
                return true;
            }
            KeyCode::Backspace => {
                self.session.terrain_lab.save_name.pop();
                return true;
            }
            _ => {}
        }

        let shift_held = self.input.key(KeyCode::ShiftLeft) || self.input.key(KeyCode::ShiftRight);
        if let Some(ch) = terrain_lab_save_name_char(code, shift_held)
            && self.session.terrain_lab.save_name.len() < MAX_TERRAIN_LAB_SAVE_NAME_CHARS
        {
            self.session.terrain_lab.save_name.push(ch);
            return true;
        }

        true
    }

    pub(super) fn handle_terrain_lab_input(&mut self, code: KeyCode, repeat: bool) -> bool {
        if !self.session.terrain_lab.enabled {
            return false;
        }

        if code == KeyCode::F8 && !repeat {
            self.session.terrain_lab.panel_visible = !self.session.terrain_lab.panel_visible;
            return true;
        }
        if code == KeyCode::F9 && !repeat {
            self.terrain_lab_save_preset();
            return true;
        }
        if !self.session.terrain_lab.panel_visible {
            return false;
        }
        if code == KeyCode::F10 && !repeat {
            self.session.terrain_lab.editing_save_name =
                !self.session.terrain_lab.editing_save_name;
            if self.session.terrain_lab.editing_save_name
                && self.session.terrain_lab.save_name.trim().is_empty()
            {
                self.session.terrain_lab.save_name =
                    default_terrain_lab_save_name(self.session.world_seed);
            }
            return true;
        }

        if self.session.terrain_lab.editing_save_name {
            return self.handle_terrain_lab_name_input(code);
        }

        let keys = TerrainConfig::parameter_keys();
        if keys.is_empty() {
            return false;
        }

        let selected = self
            .session
            .terrain_lab
            .selected_index
            .min(keys.len().saturating_sub(1));
        self.session.terrain_lab.selected_index = selected;

        match code {
            KeyCode::ArrowUp => {
                self.session.terrain_lab.selected_index = selected.saturating_sub(1);
                true
            }
            KeyCode::ArrowDown => {
                self.session.terrain_lab.selected_index = (selected + 1).min(keys.len() - 1);
                true
            }
            KeyCode::ArrowLeft => {
                let index = self.session.terrain_lab.selected_index;
                let step = self.terrain_lab_step_for(keys[index]);
                self.terrain_lab_adjust_selected(-step);
                true
            }
            KeyCode::ArrowRight => {
                let index = self.session.terrain_lab.selected_index;
                let step = self.terrain_lab_step_for(keys[index]);
                self.terrain_lab_adjust_selected(step);
                true
            }
            KeyCode::PageUp => {
                let index = self.session.terrain_lab.selected_index;
                let step = self.terrain_lab_step_for(keys[index]);
                self.terrain_lab_adjust_selected(-step * 5.0);
                true
            }
            KeyCode::PageDown => {
                let index = self.session.terrain_lab.selected_index;
                let step = self.terrain_lab_step_for(keys[index]);
                self.terrain_lab_adjust_selected(step * 5.0);
                true
            }
            _ => false,
        }
    }

    pub(super) fn handle_terrain_lab_wheel(&mut self, delta_y: f32) -> bool {
        if !self.session.terrain_lab.enabled
            || !self.session.terrain_lab.panel_visible
            || self.menu.ui_mode != UiMode::Playing
            || delta_y.abs() <= f32::EPSILON
        {
            return false;
        }
        let keys = TerrainConfig::parameter_keys();
        if keys.is_empty() {
            return false;
        }
        let direction = if delta_y > 0.0 { -1 } else { 1 };
        self.session.terrain_lab.selected_index = if direction < 0 {
            self.session.terrain_lab.selected_index.saturating_sub(1)
        } else {
            (self.session.terrain_lab.selected_index + 1).min(keys.len() - 1)
        };
        true
    }
}

pub(super) fn default_terrain_lab_save_name(seed: i64) -> String {
    format!("terrain_lab_seed_{seed}")
}

fn sanitize_terrain_recipe_name(raw: &str) -> String {
    let lowered = raw.trim().to_ascii_lowercase();
    let mapped = lowered
        .chars()
        .map(|ch| match ch {
            'a'..='z' | '0'..='9' => ch,
            '-' | '_' => ch,
            ' ' => '_',
            _ => '_',
        })
        .collect::<String>();
    let collapsed = mapped
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if collapsed.is_empty() {
        "terrain".to_string()
    } else {
        collapsed
    }
}

fn terrain_lab_save_name_char(code: KeyCode, shift_held: bool) -> Option<char> {
    match code {
        KeyCode::KeyA => Some(if shift_held { 'A' } else { 'a' }),
        KeyCode::KeyB => Some(if shift_held { 'B' } else { 'b' }),
        KeyCode::KeyC => Some(if shift_held { 'C' } else { 'c' }),
        KeyCode::KeyD => Some(if shift_held { 'D' } else { 'd' }),
        KeyCode::KeyE => Some(if shift_held { 'E' } else { 'e' }),
        KeyCode::KeyF => Some(if shift_held { 'F' } else { 'f' }),
        KeyCode::KeyG => Some(if shift_held { 'G' } else { 'g' }),
        KeyCode::KeyH => Some(if shift_held { 'H' } else { 'h' }),
        KeyCode::KeyI => Some(if shift_held { 'I' } else { 'i' }),
        KeyCode::KeyJ => Some(if shift_held { 'J' } else { 'j' }),
        KeyCode::KeyK => Some(if shift_held { 'K' } else { 'k' }),
        KeyCode::KeyL => Some(if shift_held { 'L' } else { 'l' }),
        KeyCode::KeyM => Some(if shift_held { 'M' } else { 'm' }),
        KeyCode::KeyN => Some(if shift_held { 'N' } else { 'n' }),
        KeyCode::KeyO => Some(if shift_held { 'O' } else { 'o' }),
        KeyCode::KeyP => Some(if shift_held { 'P' } else { 'p' }),
        KeyCode::KeyQ => Some(if shift_held { 'Q' } else { 'q' }),
        KeyCode::KeyR => Some(if shift_held { 'R' } else { 'r' }),
        KeyCode::KeyS => Some(if shift_held { 'S' } else { 's' }),
        KeyCode::KeyT => Some(if shift_held { 'T' } else { 't' }),
        KeyCode::KeyU => Some(if shift_held { 'U' } else { 'u' }),
        KeyCode::KeyV => Some(if shift_held { 'V' } else { 'v' }),
        KeyCode::KeyW => Some(if shift_held { 'W' } else { 'w' }),
        KeyCode::KeyX => Some(if shift_held { 'X' } else { 'x' }),
        KeyCode::KeyY => Some(if shift_held { 'Y' } else { 'y' }),
        KeyCode::KeyZ => Some(if shift_held { 'Z' } else { 'z' }),
        KeyCode::Digit0 => Some('0'),
        KeyCode::Digit1 => Some('1'),
        KeyCode::Digit2 => Some('2'),
        KeyCode::Digit3 => Some('3'),
        KeyCode::Digit4 => Some('4'),
        KeyCode::Digit5 => Some('5'),
        KeyCode::Digit6 => Some('6'),
        KeyCode::Digit7 => Some('7'),
        KeyCode::Digit8 => Some('8'),
        KeyCode::Digit9 => Some('9'),
        KeyCode::Minus => Some(if shift_held { '_' } else { '-' }),
        KeyCode::Space => Some(' '),
        _ => None,
    }
}
