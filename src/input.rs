use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
        KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
    },
    input::{
        keyboard::{xkb, FilterResult, Keysym, ModifiersState},
        pointer::{AxisFrame, ButtonEvent, MotionEvent, RelativeMotionEvent},
    },
    utils::SERIAL_COUNTER,
};

use crate::state::State;

pub enum Action {
    /// Nothing to do, for example when a keyboard event is passed to the client.
    None,
    /// Ctrl-Alt-Fx, to change tty.
    ChangeVt(i32),
    /// Ctrl-Alt-Backspace, to exit the compositor.
    Quit,
}

impl<BackendData> State<BackendData> {
    /// Whenever an input event is occurred, pass it to this function, no matter whether it is a
    /// Winit one or a Libinput one.
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
                    let action = keyboard
                        .input::<Action, _>(
                            self,
                            event.key_code(),
                            event.state(),
                            serial,
                            time,
                            |_, modifiers, handler| {
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
            // When a pointer moves, for the DRM backend.
            InputEvent::PointerMotion { event } => {
                if let Some(pointer) = self.seat.get_pointer() {
                    let new_location = pointer.current_location() + event.delta();

                    let serial = SERIAL_COUNTER.next_serial();
                    let under = self.surface_under_pointer(&pointer);

                    let output_under = self.space.output_under(new_location).next();
                    if output_under.is_none() {
                        return Action::None;
                    }

                    pointer.motion(
                        self,
                        under.clone(),
                        &MotionEvent {
                            location: new_location,
                            serial,
                            time: event.time_msec(),
                        },
                    );
                    pointer.relative_motion(
                        self,
                        under,
                        &RelativeMotionEvent {
                            delta: event.delta(),
                            delta_unaccel: event.delta_unaccel(),
                            utime: event.time(),
                        },
                    );
                }
            }
            // When a pointer moves, for the Winit backend.
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
            // When a mouse wheel rolls.
            InputEvent::PointerAxis { event } => {
                let source = event.source();

                let horizontal_amount = event.amount(Axis::Horizontal).unwrap_or_else(|| {
                    event.amount_discrete(Axis::Horizontal).unwrap_or(0.0) * 3.0
                });
                let vertical_amount = event
                    .amount(Axis::Vertical)
                    .unwrap_or_else(|| event.amount_discrete(Axis::Vertical).unwrap_or(0.0) * 3.0);

                let horizontal_amount_discrete = event.amount_discrete(Axis::Horizontal);
                let vertical_amount_discrete = event.amount_discrete(Axis::Vertical);

                let mut frame = AxisFrame::new(event.time_msec()).source(source);

                if horizontal_amount != 0.0 {
                    frame = frame.value(Axis::Horizontal, horizontal_amount);
                    if let Some(discrete) = horizontal_amount_discrete {
                        frame = frame.discrete(Axis::Horizontal, discrete as i32);
                    } else if source == AxisSource::Finger {
                        frame = frame.stop(Axis::Horizontal);
                    }
                }
                if vertical_amount != 0.0 {
                    frame = frame.value(Axis::Vertical, vertical_amount);
                    if let Some(discrete) = vertical_amount_discrete {
                        frame = frame.discrete(Axis::Vertical, discrete as i32);
                    } else if source == AxisSource::Finger {
                        frame = frame.stop(Axis::Vertical);
                    }
                }

                if let Some(pointer) = self.seat.get_pointer() {
                    pointer.axis(self, frame);
                }
            }
            // When a mouse button is pressed or released.
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

/// Checks if a keyboard shortcut is tiggered.
fn process_keyboard_shortcut(modifiers: &ModifiersState, keysym: Keysym) -> Option<Action> {
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
