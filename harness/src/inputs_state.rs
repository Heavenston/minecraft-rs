
use std::collections::HashSet;

use glam::DVec2;
pub use winit::event::MouseButton;
pub use winit::keyboard::KeyCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Input {
    Key(KeyCode),
    Button(MouseButton),
}

impl From<KeyCode> for Input {
    fn from(value: KeyCode) -> Self {
        Self::Key(value)
    }
}

impl From<MouseButton> for Input {
    fn from(value: MouseButton) -> Self {
        Self::Button(value)
    }
}

pub struct InputsState {
    just_pressed: HashSet<Input>,
    just_released: HashSet<Input>,
    pressed: HashSet<Input>,
    mouse_pos: DVec2,
    mouse_motion: DVec2,
}

impl InputsState {
    #[expect(clippy::single_call_fn, reason = "Only for winit loop handler")]
    pub(crate) fn new() -> Self {
        Self {
            just_pressed: HashSet::default(),
            just_released: HashSet::default(),
            pressed: HashSet::default(),
            mouse_pos: DVec2::ZERO,
            mouse_motion: DVec2::ZERO,
        }
    }

    pub(crate) fn frame_clear(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
        self.mouse_motion = DVec2::ZERO;
    }

    pub(crate) fn register_input_event(&mut self, input: Input, pressed: bool) {
        if pressed {
            self.just_pressed.insert(input);
            self.pressed.insert(input);
        }
        else {
            self.just_released.insert(input);
            self.pressed.remove(&input);
        }
    }

    pub(crate) fn register_mouse_pos(&mut self, pos: DVec2) {
        self.mouse_pos = pos;
    }

    pub(crate) fn register_mouse_motion(&mut self, motion: DVec2) {
        self.mouse_motion += motion;
    }

    pub fn just_pressed(&self, input: impl Into<Input>) -> bool {
        self.just_pressed.contains(&input.into())
    }

    pub fn just_released(&self, input: impl Into<Input>) -> bool {
        self.just_released.contains(&input.into())
    }

    pub fn pressed(&self, input: impl Into<Input>) -> bool {
        let input = input.into();

        // also checks just_pressed so that if a key is pressed an release
        // during a single frame, at lease one frame will say .pressed true for
        // this key
        self.just_pressed.contains(&input) || self.pressed.contains(&input)
    }

    pub fn mouse_position(&self) -> DVec2 {
        self.mouse_pos
    }

    pub fn mouse_motion(&self) -> DVec2 {
        self.mouse_motion
    }
}
