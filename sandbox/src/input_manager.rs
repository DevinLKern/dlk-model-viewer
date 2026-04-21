use std::collections::HashSet;


#[allow(unused)]
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum Input {
    Key(winit::keyboard::KeyCode),
    MouseButton(winit::event::MouseButton),
    MouseMotion,
    MouseWheelUp,
    MouseWheelDown,
}

impl Input {
    pub fn from_str(s: &str) -> Option<Self> {
        use winit::event::MouseButton;
        use winit::keyboard::KeyCode;

        let s = s.to_lowercase();
        let res = match s.as_str() {
            "a" => Input::Key(KeyCode::KeyA),
            "b" => Input::Key(KeyCode::KeyB),
            "c" => Input::Key(KeyCode::KeyC),
            "d" => Input::Key(KeyCode::KeyD),
            "e" => Input::Key(KeyCode::KeyE),
            "f" => Input::Key(KeyCode::KeyF),
            "g" => Input::Key(KeyCode::KeyG),
            "h" => Input::Key(KeyCode::KeyH),
            "i" => Input::Key(KeyCode::KeyI),
            "j" => Input::Key(KeyCode::KeyJ),
            "k" => Input::Key(KeyCode::KeyK),
            "l" => Input::Key(KeyCode::KeyL),
            "m" => Input::Key(KeyCode::KeyM),
            "n" => Input::Key(KeyCode::KeyN),
            "o" => Input::Key(KeyCode::KeyO),
            "p" => Input::Key(KeyCode::KeyP),
            "q" => Input::Key(KeyCode::KeyQ),
            "r" => Input::Key(KeyCode::KeyR),
            "s" => Input::Key(KeyCode::KeyS),
            "t" => Input::Key(KeyCode::KeyT),
            "u" => Input::Key(KeyCode::KeyU),
            "v" => Input::Key(KeyCode::KeyV),
            "w" => Input::Key(KeyCode::KeyW),
            "x" => Input::Key(KeyCode::KeyX),
            "y" => Input::Key(KeyCode::KeyY),
            "z" => Input::Key(KeyCode::KeyZ),
            "space" => Input::Key(KeyCode::Space),
            "shift_left" => Input::Key(KeyCode::ShiftLeft),
            "shift_right" => Input::Key(KeyCode::ShiftRight),
            "ctrl_left" => Input::Key(KeyCode::ControlLeft),
            "ctrl_right" => Input::Key(KeyCode::ControlRight),
            "mouse1" => Input::MouseButton(MouseButton::Left),
            "mouse2" => Input::MouseButton(MouseButton::Right),
            "0" => Input::Key(KeyCode::Digit0),
            "1" => Input::Key(KeyCode::Digit1),
            "2" => Input::Key(KeyCode::Digit2),
            "3" => Input::Key(KeyCode::Digit3),
            "4" => Input::Key(KeyCode::Digit4),
            "5" => Input::Key(KeyCode::Digit5),
            "6" => Input::Key(KeyCode::Digit6),
            "7" => Input::Key(KeyCode::Digit7),
            "8" => Input::Key(KeyCode::Digit8),
            "9" => Input::Key(KeyCode::Digit9),
            "mouse_moved" => Input::MouseMotion,
            "mousewheel_up" => Input::MouseWheelUp,
            "mousewheel_down" => Input::MouseWheelDown,
            "=" => Input::Key(KeyCode::Equal),
            "-" => Input::Key(KeyCode::Minus),
            _ => return None,
        };

        Some(res)
    }
}

#[allow(unused)]
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub struct InputInfo {
    pub input: Input,
    pub just_pressed: bool,
    pub just_released: bool,
    pub is_held: bool,
}

#[derive(Debug, Clone)]
pub enum InputEvent {
    Window(winit::event::WindowEvent),
    Device(winit::event::DeviceEvent),
}

pub struct InputManager {
    pub prev_active_inputs: std::collections::HashSet<Input>,
    pub cur_active_inputs: std::collections::HashSet<Input>,
    pub mouse_delta: (f64, f64),
    pub wheel_delta_y: f32,
}

impl InputManager {
    pub fn new() -> Self {
        Self {
            prev_active_inputs: HashSet::new(),
            cur_active_inputs: HashSet::new(),
            mouse_delta: (0.0, 0.0),
            wheel_delta_y: 0.0,
        }
    }
    // end_frame not needed?
    pub fn start_frame(&mut self) {
        self.prev_active_inputs = self.cur_active_inputs.clone();
        self.cur_active_inputs
            .remove(&Input::MouseWheelUp);
        self.cur_active_inputs
            .remove(&Input::MouseWheelDown);
        self.mouse_delta = (0.0, 0.0);
        if self.wheel_delta_y != 0.0 {
            let wheel_input = if self.wheel_delta_y > 0.0 {
                Input::MouseWheelUp
            } else {
                Input::MouseWheelDown
            };
            self.cur_active_inputs.insert(wheel_input);
            self.wheel_delta_y = 0.0;
        }
    }
    pub fn update(&mut self, event: InputEvent) {
        use winit::event::ElementState;
        use winit::event::WindowEvent;
        use winit::keyboard::PhysicalKey;

        let event = match event {
            InputEvent::Window(we) => we,
            InputEvent::Device(winit::event::DeviceEvent::MouseMotion { delta }) => {
                self.mouse_delta.0 += delta.0;
                self.mouse_delta.1 += delta.1;
                return;
            }
            InputEvent::Device(winit::event::DeviceEvent::MouseWheel { delta }) => {
                use winit::event::MouseScrollDelta;
                let y = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(delta) => delta.y as f32,
                };
                if y != 0.0 {
                    self.wheel_delta_y += y;
                }
                return;
            }
            _ => return,
        };

        let (input, event_state) = match event {
            WindowEvent::MouseInput { button, state, .. } => (Input::MouseButton(button), state),
            WindowEvent::KeyboardInput { event, .. } => {
                let code = match event.physical_key {
                    PhysicalKey::Code(c) => c,
                    _ => return,
                };
                (Input::Key(code), event.state)
            }
            WindowEvent::Focused(b) => {
                if !b {
                    self.cur_active_inputs.clear();
                }
                return;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                use winit::event::MouseScrollDelta;
                let y = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(d) => d.y as f32,
                };
                if y != 0.0 {
                    self.wheel_delta_y += y;
                }
                return;
            }
            _ => return,
        };

        match event_state {
            ElementState::Pressed => {
                self.cur_active_inputs.insert(input);
            }
            ElementState::Released => {
                self.cur_active_inputs.remove(&input);
            }
        };
    }
    pub fn is_held(&self, input: &Input) -> bool {
        self.cur_active_inputs.contains(input)
    }
    pub fn just_pressed(&self, input: &Input) -> bool {
        self.cur_active_inputs.contains(input) && !self.prev_active_inputs.contains(input)
    }
    pub fn just_released(&self, input: &Input) -> bool {
        !self.cur_active_inputs.contains(input) && self.prev_active_inputs.contains(input)
    }
    pub fn all_just_pressed(&self) -> impl Iterator<Item = &Input> {
        self.cur_active_inputs
            .iter()
            .filter_map(|input| -> Option<&Input> {
                if self.prev_active_inputs.contains(input) {
                    return None;
                }
                Some(input)
            })
    }
}
