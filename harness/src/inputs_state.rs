
use std::collections::HashSet;

pub use winit::keyboard::KeyCode;

pub struct InputsState {
    just_pressed_keys: HashSet<KeyCode>,
    just_released_keys: HashSet<KeyCode>,
    pressed_keys: HashSet<KeyCode>,
}

impl InputsState {
    pub(crate) fn new() -> Self {
        Self {
            just_pressed_keys: Default::default(),
            just_released_keys: Default::default(),
            pressed_keys: Default::default(),
        }
    }

    pub(crate) fn clear_just_pressed_keys(&mut self) {
        self.just_pressed_keys.clear();
        self.just_released_keys.clear();
    }

    pub(crate) fn register_key_event(&mut self, keycode: KeyCode, pressed: bool) {
        if pressed {
            self.just_pressed_keys.insert(keycode);
            self.pressed_keys.insert(keycode);
        }
        else {
            self.just_released_keys.insert(keycode);
            self.pressed_keys.remove(&keycode);
        }
    }

    pub fn just_pressed(&self, code: KeyCode) -> bool {
        self.just_pressed_keys.contains(&code)
    }

    pub fn just_released(&self, code: KeyCode) -> bool {
        self.just_released_keys.contains(&code)
    }

    pub fn pressed(&self, code: KeyCode) -> bool {
        // also checks just_pressed_keys so that if a key is pressed an release
        // during a single frame, at lease one frame will say .pressed true for
        // this key
        self.just_pressed_keys.contains(&code) || self.pressed_keys.contains(&code)
    }
}
