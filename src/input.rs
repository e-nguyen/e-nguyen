// This program is free software: you can redistribute it and/or modify
// it under the terms of the Lesser GNU General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// Lesser GNU General Public License for more details.

// You should have received a copy of the Lesser GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

// Copyright 2019 E-Nguyen Developers.

use winit::ElementState::{Pressed, Released};
use winit::{DeviceEvent, Event, KeyboardInput, ModifiersState, WindowEvent};

#[derive(Clone, Debug)]
pub enum MousePos {
    NoPos,
    Pos { x: f64, y: f64 },
}

#[derive(Debug)]
pub enum UserEvent {
    // Abstract, useful events
    MouseMove { pos: MousePos },
    MouseDown { pos: MousePos },
    MouseUp { pos: MousePos },
    KeyPress { character: char },
}

pub enum ProcessedEvent {
    // Raw events require too much digging to write logic
    LeftButtonDown { no_mods: bool },
    LeftButtonUp,
    CursorMove { x: f64, y: f64 },
    CursorLeft,
    CursorEntered,
    KeyDown { scancode: u32, no_mods: bool },
    KeyUp { scancode: u32, no_mods: bool },
    KeyChar { character: char },
}

pub struct MouseTracker {
    mouse_down: MousePos,
    last_pos: MousePos,
    in_bounds: bool, // not in great use
}

impl MouseTracker {
    pub fn new() -> MouseTracker {
        MouseTracker { mouse_down: MousePos::NoPos, last_pos: MousePos::NoPos, in_bounds: true }
    }

    pub fn update(&mut self, event: &ProcessedEvent) -> Option<UserEvent> {
        match event {
            ProcessedEvent::LeftButtonDown { no_mods, .. } => {
                if *no_mods {
                    if let MousePos::Pos { x, y } = self.last_pos {
                        self.mouse_down = MousePos::Pos { x, y };
                        Some(UserEvent::MouseDown { pos: self.last_pos.clone() })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            ProcessedEvent::LeftButtonUp => {
                let mut mouse_down = MousePos::NoPos;
                std::mem::swap(&mut mouse_down, &mut self.mouse_down);
                if let MousePos::Pos { x, y } = self.last_pos {
                    if let MousePos::Pos { .. } = mouse_down {
                        Some(UserEvent::MouseUp { pos: MousePos::Pos { x, y } })
                    } else {
                        Some(UserEvent::MouseUp { pos: MousePos::NoPos })
                    }
                } else {
                    Some(UserEvent::MouseUp { pos: MousePos::NoPos })
                }
            }
            ProcessedEvent::CursorLeft => {
                self.in_bounds = false;
                self.last_pos = MousePos::NoPos;
                None
            }
            ProcessedEvent::CursorEntered => {
                self.in_bounds = true;
                None
            }
            ProcessedEvent::CursorMove { x, y } => {
                self.last_pos = MousePos::Pos { x: *x, y: *y };;
                Some(UserEvent::MouseMove { pos: self.last_pos.clone() })
            }
            _ => None,
        }
    }
}

#[derive(Debug)]
enum LastKey {
    CodeOnly { code: u32 },
    KeyCode { key: char, code: u32 },
    NoKey,
}

pub struct KeyTracker {
    key_down: LastKey,
}

impl KeyTracker {
    pub fn update(&mut self, event: &ProcessedEvent) -> Option<UserEvent> {
        match event {
            // ignore all modkey release combinations to avoid hotkey interference
            ProcessedEvent::KeyUp { no_mods: false, .. } => {
                self.key_down = LastKey::NoKey;
                None
            }
            ProcessedEvent::KeyUp { scancode, .. } => {
                if let LastKey::KeyCode { code: last_code, key } = self.key_down {
                    self.key_down = LastKey::NoKey;
                    if last_code == *scancode {
                        Some(UserEvent::KeyPress { character: key })
                    } else {
                        None
                    }
                } else {
                    self.key_down = LastKey::NoKey;
                    None
                }
            }
            ProcessedEvent::KeyChar { character } => {
                if let LastKey::CodeOnly { code } = self.key_down {
                    self.key_down = LastKey::KeyCode { code, key: *character };
                }
                None
            }
            ProcessedEvent::KeyDown { scancode, no_mods } => {
                if *no_mods {
                    self.key_down = LastKey::CodeOnly { code: *scancode };
                } else {
                    self.key_down = LastKey::NoKey;
                }
                None
            }
            _ => None,
        }
    }

    pub fn new() -> KeyTracker {
        KeyTracker { key_down: LastKey::NoKey }
    }
}

pub fn process(ev: &winit::Event) -> Option<ProcessedEvent> {
    match &ev {
        Event::DeviceEvent { event, .. } => match event {
            DeviceEvent::Button { state: Released, button: 1, .. } => {
                Some(ProcessedEvent::LeftButtonUp)
            }
            _ => None,
        },
        Event::WindowEvent { event, .. } => match event {
            //WindowEvent { event: MouseInput { state: Released, button: Left, modifiers: ModifiersState { shift: false, ctrl: false, alt: false, logo: false } } }
            WindowEvent::MouseInput { state: Pressed, button: Left, .. } => {
                Some(ProcessedEvent::LeftButtonDown { no_mods: mod_keys_off(&ev) })
            }
            WindowEvent::KeyboardInput {
                input: KeyboardInput { state: Released, scancode, .. },
                ..
            } => Some(ProcessedEvent::KeyUp { scancode: *scancode, no_mods: mod_keys_off(&ev) }),
            WindowEvent::KeyboardInput {
                input: KeyboardInput { state: Pressed, scancode, .. },
                ..
            } => Some(ProcessedEvent::KeyDown { scancode: *scancode, no_mods: mod_keys_off(&ev) }),
            WindowEvent::ReceivedCharacter(c) => Some(ProcessedEvent::KeyChar { character: *c }),
            WindowEvent::CursorMoved { position, .. } => {
                Some(ProcessedEvent::CursorMove { x: position.x, y: position.y })
            }
            WindowEvent::CursorLeft { .. } => Some(ProcessedEvent::CursorLeft),
            WindowEvent::CursorEntered { .. } => Some(ProcessedEvent::CursorEntered),
            _ => None,
        },
        _ => None,
    }
}

fn mod_keys_off(ev: &Event) -> bool {
    match ev {
        Event::WindowEvent {
            event:
                WindowEvent::KeyboardInput {
                    input:
                        KeyboardInput {
                            modifiers:
                                ModifiersState { shift: false, ctrl: false, alt: false, logo: false },
                            ..
                        },
                    ..
                },
            ..
        } => true,
        Event::WindowEvent {
            event:
                WindowEvent::MouseInput {
                    modifiers:
                        ModifiersState { shift: false, ctrl: false, alt: false, logo: false, .. },
                    ..
                },
            ..
        } => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_tracking() {
        let mut kt = KeyTracker::new();
        let inputs = vec![
            // yields one UserEvent::KeyPress{'c'}
            ProcessedEvent::KeyDown { scancode: 0, no_mods: true },
            ProcessedEvent::KeyChar { character: 'c' },
            ProcessedEvent::KeyUp { scancode: 0, no_mods: true },
            // resets internals so that KeyUp fails to yield a key
            ProcessedEvent::KeyDown { scancode: 1, no_mods: true },
            ProcessedEvent::KeyChar { character: 'c' },
            ProcessedEvent::KeyDown { scancode: 2, no_mods: true },
            ProcessedEvent::KeyChar { character: 'a' },
            ProcessedEvent::KeyUp { scancode: 0, no_mods: true },
            // fails to yield again on final KeyUp with modifiers
            ProcessedEvent::KeyDown { scancode: 7, no_mods: true },
            ProcessedEvent::KeyChar { character: 'z' },
            ProcessedEvent::KeyUp { scancode: 7, no_mods: false },
            // yields one UserEvent::Keypress{'m'}
            ProcessedEvent::KeyDown { scancode: 8, no_mods: true },
            ProcessedEvent::KeyChar { character: 'm' },
            ProcessedEvent::KeyUp { scancode: 8, no_mods: true },
            // fails to yield if KeyUp code is wrong
            ProcessedEvent::KeyDown { scancode: 12, no_mods: true },
            ProcessedEvent::KeyChar { character: 'q' },
            ProcessedEvent::KeyUp { scancode: 24, no_mods: true },
        ];
        let mut outputs: Vec<UserEvent> = inputs.iter().filter_map(|m| kt.update(&m)).collect();
        assert_eq!(2, outputs.len());
        let t1 = outputs.pop().unwrap();
        match t1 {
            UserEvent::KeyPress { character } => assert_eq!('m', character),
            _ => panic!("mismatched"),
        }
        let t2 = outputs.pop().unwrap();
        match t2 {
            UserEvent::KeyPress { character } => assert_eq!('c', character),
            _ => panic!("mismatched"),
        }
    }

    #[test]
    fn test_key_tracker_internals() {
        let mut kt = KeyTracker::new();
        kt.update(&ProcessedEvent::KeyDown { scancode: 0, no_mods: true });
        let last_code = if let LastKey::CodeOnly { code } = kt.key_down { code } else { 666 };
        assert_eq!(0, last_code);
        kt.update(&ProcessedEvent::KeyDown { scancode: 0, no_mods: false });
        let no_key = if let LastKey::NoKey = kt.key_down { true } else { false };
        assert!(no_key);
    }

    #[test]
    fn test_mouse_tracking() {
        let mut mt = MouseTracker::new();
        let inputs = vec![
            // yields MouseDown at 10, 20 and MouseUp at 20, 30
            ProcessedEvent::CursorMove { x: 10., y: 20. },
            ProcessedEvent::LeftButtonDown { no_mods: true },
            ProcessedEvent::CursorMove { x: 20., y: 30. },
            ProcessedEvent::LeftButtonUp,
            // yields MouseDown at same location and MouseUp with MousePos::NoPos
            ProcessedEvent::LeftButtonDown { no_mods: true },
            ProcessedEvent::CursorLeft,
            ProcessedEvent::LeftButtonUp,
            // does not cause MouseDown as it must be outside window
            ProcessedEvent::LeftButtonDown { no_mods: true },
            // does not cause MousDown due to modifier keys
            ProcessedEvent::CursorMove { x: 30., y: 40. },
            ProcessedEvent::LeftButtonDown { no_mods: false },
        ];
        let mut outputs: Vec<UserEvent> = inputs.iter().filter_map(|m| mt.update(&m)).collect();
        assert_eq!(7, outputs.len());
        outputs.reverse();
        let mut m = outputs.pop().unwrap();
        match m {
            UserEvent::MouseMove { pos: MousePos::Pos { x, y } } => {
                assert_eq!((10., 20.), (x, y));
            }
            _ => panic!("mismatched"),
        }
        m = outputs.pop().unwrap();
        match m {
            UserEvent::MouseDown { pos: MousePos::Pos { x, y } } => {
                assert_eq!((10., 20.), (x, y));
            }
            _ => panic!("mismatched"),
        }
        m = outputs.pop().unwrap();
        match m {
            UserEvent::MouseMove { pos: MousePos::Pos { x, y } } => {
                assert_eq!((20., 30.), (x, y));
            }
            _ => panic!("mismatched"),
        }
        m = outputs.pop().unwrap();
        match m {
            UserEvent::MouseUp { pos: MousePos::Pos { x, y } } => {
                assert_eq!((20., 30.), (x, y));
            }
            _ => panic!("mismatched"),
        }
        m = outputs.pop().unwrap();
        match m {
            UserEvent::MouseDown { pos: MousePos::Pos { x, y } } => {
                assert_eq!((20., 30.), (x, y));
            }
            _ => panic!("mismatched"),
        }
        m = outputs.pop().unwrap();
        match m {
            UserEvent::MouseUp { pos: MousePos::NoPos } => { /* dont panic */ }
            _ => panic!("mismatched"),
        }
        m = outputs.pop().unwrap();
        match m {
            UserEvent::MouseMove { pos: MousePos::Pos { x, y } } => {
                assert_eq!((30., 40.), (x, y));
            }
            _ => panic!("mismatched"),
        }
    }
}
