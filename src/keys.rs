use eframe::egui::{Context, Event, Key, Modifiers};
use eframe::emath::Vec2;

use crate::{
    message::Message,
    wave_data::{PER_SCROLL_EVENT, SCROLL_EVENTS_PER_PAGE},
    MoveDir, State,
};

impl State {
    pub fn handle_pressed_keys(&self, ctx: &Context, msgs: &mut Vec<Message>) {
        ctx.input(|i| {
            i.events.iter().for_each(|event| match event {
                Event::Key {
                    key,
                    repeat: _,
                    pressed,
                    modifiers,
                    physical_key: _,
                } => match (
                    key,
                    pressed,
                    self.sys.command_prompt.visible,
                    self.variable_name_filter_focused,
                ) {
                    (Key::Num0, true, false, false) => {
                        handle_digit(0, modifiers, msgs);
                    }
                    (Key::Num1, true, false, false) => {
                        handle_digit(1, modifiers, msgs);
                    }
                    (Key::Num2, true, false, false) => {
                        handle_digit(2, modifiers, msgs);
                    }
                    (Key::Num3, true, false, false) => {
                        handle_digit(3, modifiers, msgs);
                    }
                    (Key::Num4, true, false, false) => {
                        handle_digit(4, modifiers, msgs);
                    }
                    (Key::Num5, true, false, false) => {
                        handle_digit(5, modifiers, msgs);
                    }
                    (Key::Num6, true, false, false) => {
                        handle_digit(6, modifiers, msgs);
                    }
                    (Key::Num7, true, false, false) => {
                        handle_digit(7, modifiers, msgs);
                    }
                    (Key::Num8, true, false, false) => {
                        handle_digit(8, modifiers, msgs);
                    }
                    (Key::Num9, true, false, false) => {
                        handle_digit(9, modifiers, msgs);
                    }
                    (Key::Home, true, false, false) => msgs.push(Message::ScrollToItem(0)),
                    (Key::End, true, false, false) => {
                        if let Some(waves) = &self.waves {
                            if waves.displayed_items.len() > 1 {
                                msgs.push(Message::ScrollToItem(waves.displayed_items.len() - 1));
                            }
                        }
                    }
                    (Key::Space, true, false, false) => msgs.push(Message::ShowCommandPrompt(true)),
                    (Key::Escape, true, true, false) => {
                        msgs.push(Message::ShowCommandPrompt(false))
                    }
                    (Key::G, true, true, false) => {
                        if modifiers.ctrl {
                            msgs.push(Message::ShowCommandPrompt(false))
                        }
                    }
                    (Key::Escape, true, false, false) => msgs.push(Message::InvalidateCount),
                    (Key::Escape, true, _, true) => msgs.push(Message::SetFilterFocused(false)),
                    (Key::B, true, false, false) => msgs.push(Message::ToggleSidePanel),
                    (Key::M, true, false, false) => msgs.push(Message::ToggleMenu),
                    (Key::T, true, false, false) => msgs.push(Message::ToggleToolbar),
                    (Key::F11, true, false, _) => msgs.push(Message::ToggleFullscreen),
                    (Key::S, true, false, false) => msgs.push(Message::GoToStart),
                    (Key::E, true, false, false) => msgs.push(Message::GoToEnd),
                    (Key::R, true, false, false) => msgs.push(Message::ReloadWaveform(
                        self.config.behavior.keep_during_reload,
                    )),
                    (Key::Minus, true, false, false) => msgs.push(Message::CanvasZoom {
                        mouse_ptr_timestamp: None,
                        delta: 2.0,
                    }),
                    (Key::Plus | Key::Equals, true, false, false) => {
                        msgs.push(Message::CanvasZoom {
                            mouse_ptr_timestamp: None,
                            delta: 0.5,
                        })
                    }
                    (Key::PageUp, true, false, false) => msgs.push(Message::CanvasScroll {
                        delta: Vec2 {
                            x: 0.,
                            y: -PER_SCROLL_EVENT * SCROLL_EVENTS_PER_PAGE,
                        },
                    }),
                    (Key::PageDown, true, false, false) => msgs.push(Message::CanvasScroll {
                        delta: Vec2 {
                            x: 0.,
                            y: PER_SCROLL_EVENT * SCROLL_EVENTS_PER_PAGE,
                        },
                    }),
                    (Key::ArrowRight, true, false, false) => msgs.push(Message::CanvasScroll {
                        delta: Vec2 {
                            x: 0.,
                            y: -PER_SCROLL_EVENT,
                        },
                    }),
                    (Key::ArrowLeft, true, false, false) => msgs.push(Message::CanvasScroll {
                        delta: Vec2 {
                            x: 0.,
                            y: PER_SCROLL_EVENT,
                        },
                    }),
                    (Key::J, true, false, false) => {
                        if modifiers.alt {
                            msgs.push(Message::MoveFocus(MoveDir::Down, self.get_count()));
                        } else if modifiers.ctrl {
                            msgs.push(Message::MoveFocusedItem(MoveDir::Down, self.get_count()));
                        } else {
                            msgs.push(Message::VerticalScroll(MoveDir::Down, self.get_count()));
                        }
                        msgs.push(Message::InvalidateCount);
                    }
                    (Key::K, true, false, false) => {
                        if modifiers.alt {
                            msgs.push(Message::MoveFocus(MoveDir::Up, self.get_count()));
                        } else if modifiers.ctrl {
                            msgs.push(Message::MoveFocusedItem(MoveDir::Up, self.get_count()));
                        } else {
                            msgs.push(Message::VerticalScroll(MoveDir::Up, self.get_count()));
                        }
                        msgs.push(Message::InvalidateCount);
                    }
                    (Key::ArrowDown, true, false, false) => {
                        if modifiers.alt {
                            msgs.push(Message::MoveFocus(MoveDir::Down, self.get_count()));
                        } else if modifiers.ctrl {
                            msgs.push(Message::MoveFocusedItem(MoveDir::Down, self.get_count()));
                        } else {
                            msgs.push(Message::VerticalScroll(MoveDir::Down, self.get_count()));
                        }
                        msgs.push(Message::InvalidateCount);
                    }
                    (Key::ArrowUp, true, false, false) => {
                        if modifiers.alt {
                            msgs.push(Message::MoveFocus(MoveDir::Up, self.get_count()));
                        } else if modifiers.ctrl {
                            msgs.push(Message::MoveFocusedItem(MoveDir::Up, self.get_count()));
                        } else {
                            msgs.push(Message::VerticalScroll(MoveDir::Up, self.get_count()));
                        }
                        msgs.push(Message::InvalidateCount);
                    }
                    (Key::Delete | Key::X, true, false, false) => {
                        if let Some(waves) = &self.waves {
                            if let Some(idx) = waves.focused_item {
                                msgs.push(Message::RemoveItem(idx, self.get_count()));
                                msgs.push(Message::InvalidateCount);
                            }
                        }
                    }
                    (Key::ArrowUp, true, true, false) => msgs.push(Message::SelectPrevCommand),
                    (Key::P, true, true, false) => {
                        if modifiers.ctrl {
                            msgs.push(Message::SelectPrevCommand);
                        }
                    }
                    (Key::ArrowDown, true, true, false) => msgs.push(Message::SelectNextCommand),
                    (Key::N, true, true, false) => {
                        if modifiers.ctrl {
                            msgs.push(Message::SelectNextCommand);
                        }
                    }
                    _ => {}
                },
                _ => {}
            })
        });
    }

    pub fn get_count(&self) -> usize {
        if let Some(count) = &self.count {
            count.parse::<usize>().unwrap_or(1)
        } else {
            1
        }
    }
}

fn handle_digit(digit: u8, modifiers: &Modifiers, msgs: &mut Vec<Message>) {
    if modifiers.alt {
        msgs.push(Message::AddCount((digit + 48) as char))
    } else if modifiers.ctrl {
        msgs.push(Message::MoveMarkerToCursor(digit))
    } else {
        msgs.push(Message::GoToMarkerPosition(digit))
    }
}
