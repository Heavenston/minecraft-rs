
use std::collections::HashSet;

pub use winit::keyboard::KeyCode;

pub struct InputsState {
    just_pressed: HashSet<KeyCode>,
    just_released: HashSet<KeyCode>,
    pressed: HashSet<KeyCode>,
}

impl InputsState {
    #[expect(clippy::single_call_fn, reason = "Only for winit loop handler")]
    pub(crate) fn new() -> Self {
        Self {
            just_pressed: HashSet::default(),
            just_released: HashSet::default(),
            pressed: HashSet::default(),
        }
    }

    pub(crate) fn clear_just_pressed_keys(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
    }

    pub(crate) fn register_key_event(&mut self, keycode: KeyCode, pressed: bool) {
        if pressed {
            self.just_pressed.insert(keycode);
            self.pressed.insert(keycode);
        }
        else {
            self.just_released.insert(keycode);
            self.pressed.remove(&keycode);
        }
    }

    pub fn just_pressed(&self, code: KeyCode) -> bool {
        self.just_pressed.contains(&code)
    }

    pub fn just_released(&self, code: KeyCode) -> bool {
        self.just_released.contains(&code)
    }

    pub fn pressed(&self, code: KeyCode) -> bool {
        // also checks just_pressed_keys so that if a key is pressed an release
        // during a single frame, at lease one frame will say .pressed true for
        // this key
        self.just_pressed.contains(&code) || self.pressed.contains(&code)
    }
}
