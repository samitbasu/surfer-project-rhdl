//! Keyboard handling.
use egui::{Context, Event, Key, Modifiers};
use emath::Vec2;

use crate::config::ArrowKeyBindings;
use crate::displayed_item::DisplayedItemIndex;
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
                    (Key::Space, true, false, false) => {
                        msgs.push(Message::ShowCommandPrompt(Some("".to_string())))
                    }
                    (Key::Escape, true, true, false) => msgs.push(Message::ShowCommandPrompt(None)),
                    (Key::G, true, true, false) => {
                        if modifiers.command {
                            msgs.push(Message::ShowCommandPrompt(None))
                        }
                    }
                    (Key::Escape, true, false, false) => {
                        msgs.push(Message::InvalidateCount);
                        msgs.push(Message::ItemSelectionClear);
                    }
                    (Key::Escape, true, _, true) => msgs.push(Message::SetFilterFocused(false)),
                    (Key::B, true, false, false) => msgs.push(Message::ToggleSidePanel),
                    (Key::M, true, false, false) => msgs.push(Message::ToggleMenu),
                    (Key::T, true, false, false) => msgs.push(Message::ToggleToolbar),
                    (Key::F11, true, false, _) => msgs.push(Message::ToggleFullscreen),
                    (Key::U, true, false, false) => {
                        if modifiers.shift {
                            msgs.push(Message::Redo(self.get_count()));
                        } else {
                            msgs.push(Message::Undo(self.get_count()));
                        }
                    }
                    (Key::Z, true, false, false) => {
                        if modifiers.ctrl {
                            msgs.push(Message::Undo(self.get_count()));
                        }
                    }
                    (Key::Y, true, false, false) => {
                        if modifiers.ctrl {
                            msgs.push(Message::Redo(self.get_count()));
                        }
                    }
                    (Key::F, true, false, false) => {
                        msgs.push(Message::ShowCommandPrompt(Some("item_focus ".to_string())))
                    }
                    (Key::S, true, false, false) => {
                        msgs.push(Message::GoToStart { viewport_idx: 0 });
                    }
                    (Key::A, true, false, false) => {
                        if modifiers.command {
                            msgs.push(Message::Batch(vec![
                                Message::FocusItem(DisplayedItemIndex(0)),
                                Message::ItemSelectRange(DisplayedItemIndex(
                                    self.waves
                                        .as_ref()
                                        .map_or(0, |w| w.displayed_items_order.len() - 1),
                                )),
                                Message::UnfocusItem,
                            ]));
                        } else {
                            msgs.push(Message::ToggleItemSelected(None));
                        }
                    }
                    (Key::E, true, false, false) => msgs.push(Message::GoToEnd { viewport_idx: 0 }),
                    (Key::R, true, false, false) => msgs.push(Message::ReloadWaveform(
                        self.config.behavior.keep_during_reload,
                    )),
                    (Key::H, true, false, false) => msgs.push(Message::MoveCursorToTransition {
                        next: false,
                        variable: None,
                        skip_zero: modifiers.shift,
                    }),
                    (Key::L, true, false, false) => msgs.push(Message::MoveCursorToTransition {
                        next: true,
                        variable: None,
                        skip_zero: modifiers.shift,
                    }),
                    (Key::Minus, true, false, false) => msgs.push(Message::CanvasZoom {
                        mouse_ptr: None,
                        delta: 2.0,
                        viewport_idx: 0,
                    }),
                    (Key::Plus | Key::Equals, true, false, false) => {
                        msgs.push(Message::CanvasZoom {
                            mouse_ptr: None,
                            delta: 0.5,
                            viewport_idx: 0,
                        });
                    }
                    (Key::PageUp, true, false, false) => msgs.push(Message::CanvasScroll {
                        delta: Vec2 {
                            x: 0.,
                            y: -PER_SCROLL_EVENT * SCROLL_EVENTS_PER_PAGE,
                        },
                        viewport_idx: 0,
                    }),
                    (Key::PageDown, true, false, false) => msgs.push(Message::CanvasScroll {
                        delta: Vec2 {
                            x: 0.,
                            y: PER_SCROLL_EVENT * SCROLL_EVENTS_PER_PAGE,
                        },
                        viewport_idx: 0,
                    }),
                    (Key::ArrowRight, true, false, false) => {
                        msgs.push(match self.config.behavior.arrow_key_bindings {
                            ArrowKeyBindings::Edge => Message::MoveCursorToTransition {
                                next: true,
                                variable: None,
                                skip_zero: modifiers.shift,
                            },
                            ArrowKeyBindings::Scroll => Message::CanvasScroll {
                                delta: Vec2 {
                                    x: 0.,
                                    y: -PER_SCROLL_EVENT,
                                },
                                viewport_idx: 0,
                            },
                        });
                    }
                    (Key::ArrowLeft, true, false, false) => {
                        msgs.push(match self.config.behavior.arrow_key_bindings {
                            ArrowKeyBindings::Edge => Message::MoveCursorToTransition {
                                next: false,
                                variable: None,
                                skip_zero: modifiers.shift,
                            },
                            ArrowKeyBindings::Scroll => Message::CanvasScroll {
                                delta: Vec2 {
                                    x: 0.,
                                    y: PER_SCROLL_EVENT,
                                },
                                viewport_idx: 0,
                            },
                        });
                    }
                    (Key::J, true, false, false) => {
                        if modifiers.alt {
                            msgs.push(Message::MoveFocus(
                                MoveDir::Down,
                                self.get_count(),
                                modifiers.shift,
                            ));
                        } else if modifiers.command {
                            msgs.push(Message::MoveFocusedItem(MoveDir::Down, self.get_count()));
                        } else {
                            msgs.push(Message::VerticalScroll(MoveDir::Down, self.get_count()));
                        }
                        msgs.push(Message::InvalidateCount);
                    }
                    (Key::K, true, false, false) => {
                        if modifiers.alt {
                            msgs.push(Message::MoveFocus(
                                MoveDir::Up,
                                self.get_count(),
                                modifiers.shift,
                            ));
                        } else if modifiers.command {
                            msgs.push(Message::MoveFocusedItem(MoveDir::Up, self.get_count()));
                        } else {
                            msgs.push(Message::VerticalScroll(MoveDir::Up, self.get_count()));
                        }
                        msgs.push(Message::InvalidateCount);
                    }
                    (Key::ArrowDown, true, false, false) => {
                        if modifiers.alt {
                            msgs.push(Message::MoveFocus(
                                MoveDir::Down,
                                self.get_count(),
                                modifiers.shift,
                            ));
                        } else if modifiers.command {
                            msgs.push(Message::MoveFocusedItem(MoveDir::Down, self.get_count()));
                        } else {
                            msgs.push(Message::VerticalScroll(MoveDir::Down, self.get_count()));
                        }
                        msgs.push(Message::InvalidateCount);
                    }
                    (Key::ArrowUp, true, false, false) => {
                        if modifiers.alt {
                            msgs.push(Message::MoveFocus(
                                MoveDir::Up,
                                self.get_count(),
                                modifiers.shift,
                            ));
                        } else if modifiers.command {
                            msgs.push(Message::MoveFocusedItem(MoveDir::Up, self.get_count()));
                        } else {
                            msgs.push(Message::VerticalScroll(MoveDir::Up, self.get_count()));
                        }
                        msgs.push(Message::InvalidateCount);
                    }
                    (Key::Delete | Key::X, true, false, false) => {
                        if let Some(waves) = &self.waves {
                            let mut remove_ids =
                                waves.selected_items.clone().into_iter().collect::<Vec<_>>();
                            if let Some(focus) = waves.focused_item {
                                remove_ids.append(&mut vec![waves.displayed_items_order[focus.0]]);
                            }
                            msgs.push(Message::RemoveItems(remove_ids));
                        }
                    }
                    (Key::ArrowUp, true, true, false) => msgs.push(Message::SelectPrevCommand),
                    (Key::P, true, true, false) => {
                        if modifiers.command {
                            msgs.push(Message::SelectPrevCommand);
                        }
                    }
                    (Key::ArrowDown, true, true, false) => msgs.push(Message::SelectNextCommand),
                    (Key::N, true, true, false) => {
                        if modifiers.command {
                            msgs.push(Message::SelectNextCommand);
                        }
                    }
                    _ => {}
                },
                Event::Copy => msgs.push(Message::VariableValueToClipbord(None)),
                _ => {}
            });
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
        msgs.push(Message::AddCount((digit + 48) as char));
    } else if modifiers.command {
        msgs.push(Message::MoveMarkerToCursor(digit));
    } else {
        msgs.push(Message::GoToMarkerPosition(digit, 0));
    }
}
