use smithay::{
    backend::input::{
        AbsolutePositionEvent, ButtonState, Event, InputBackend, InputEvent, KeyboardKeyEvent,
        PointerButtonEvent,
    },
    input::{
        keyboard::{xkb, FilterResult, Keysym, ModifiersState},
        pointer::{ButtonEvent, MotionEvent},
    },
    utils::SERIAL_COUNTER,
};

use crate::state::State;

pub enum Action {
    None,
    ChangeVt(i32),
    Quit,
}

impl<BackendData> State<BackendData> {
    pub fn handle_input<B>(&mut self, event: InputEvent<B>) -> Action
    where
        B: InputBackend,
    {
        match event {
            // Handle keyboard events.
            InputEvent::Keyboard { event } => {
                // Currently keyboard events are forwarded to clients.
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);

                if let Some(keyboard) = self.seat.get_keyboard() {
                    let modifiers = keyboard.modifier_state();
                    let action = keyboard
                        .input::<Action, _>(
                            self,
                            event.key_code(),
                            event.state(),
                            serial,
                            time,
                            |_, _, handler| {
                                let sym = handler.modified_sym();
                                if let Some(action) = process_keyboard_shortcut(modifiers, sym) {
                                    return FilterResult::Intercept(action);
                                }
                                FilterResult::Forward
                            },
                        )
                        .unwrap_or(Action::None);
                    return action;
                }
            }
            InputEvent::PointerMotionAbsolute { event } => {
                if let Some(pointer) = self.seat.get_pointer() {
                    let output = match self.space.output_under(pointer.current_location()).next() {
                        Some(output) => output.clone(),
                        None => return Action::None,
                    };
                    let output_geo = self.space.output_geometry(&output).unwrap();
                    let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();

                    let serial = SERIAL_COUNTER.next_serial();
                    let under = self.surface_under_pointer(&pointer);
                    pointer.motion(
                        self,
                        under,
                        &MotionEvent {
                            location: pos,
                            serial,
                            time: event.time_msec(),
                        },
                    );
                }
            }
            InputEvent::PointerButton { event } => {
                if let Some(pointer) = self.seat.get_pointer() {
                    let button = event.button_code();
                    let button_state = event.state();
                    let serial = SERIAL_COUNTER.next_serial();

                    if button_state == ButtonState::Pressed && !pointer.is_grabbed() {
                        if let Some(window) = self
                            .space
                            .element_under(pointer.current_location())
                            .map(|(w, _)| w.clone())
                        {
                            // Show the clicked window on the top.
                            self.space.raise_element(&window, true);
                            // If there is a keyboard, set its focus to the clicked window.
                            if let Some(keyboard) = self.seat.get_keyboard() {
                                keyboard.set_focus(
                                    self,
                                    Some(window.toplevel().wl_surface().clone()),
                                    serial,
                                );
                            }
                            for window in self.space.elements() {
                                window.toplevel().send_pending_configure();
                            }
                        } else {
                            for window in self.space.elements() {
                                window.set_activated(false);
                                window.toplevel().send_pending_configure();
                            }
                            if let Some(keyboard) = self.seat.get_keyboard() {
                                keyboard.set_focus(self, None, serial);
                            }
                        }
                    }
                    pointer.button(
                        self,
                        &ButtonEvent {
                            button,
                            state: button_state,
                            serial,
                            time: event.time_msec(),
                        },
                    );
                }
            }
            _ => (),
        }
        Action::None
    }
}

fn process_keyboard_shortcut(modifiers: ModifiersState, keysym: Keysym) -> Option<Action> {
    if keysym == xkb::KEY_BackSpace && modifiers.ctrl && modifiers.alt {
        Some(Action::Quit)
    } else if (xkb::KEY_XF86Switch_VT_1..=xkb::KEY_XF86Switch_VT_12).contains(&keysym) {
        Some(Action::ChangeVt(
            (keysym - xkb::KEY_XF86Switch_VT_1 + 1) as i32,
        ))
    } else {
        None
    }
}
