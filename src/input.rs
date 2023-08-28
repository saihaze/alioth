use smithay::{
    backend::input::{
        AbsolutePositionEvent, ButtonState, Event, InputBackend, InputEvent, KeyboardKeyEvent,
        PointerButtonEvent,
    },
    input::{
        keyboard::FilterResult,
        pointer::{ButtonEvent, MotionEvent},
    },
    utils::SERIAL_COUNTER,
};

use crate::state::State;

impl State {
    pub fn handle_input<B>(&mut self, event: InputEvent<B>)
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
                    keyboard.input::<(), _>(
                        self,
                        event.key_code(),
                        event.state(),
                        serial,
                        time,
                        |_, _, _| FilterResult::Forward,
                    );
                }
            }
            InputEvent::PointerMotionAbsolute { event } => {
                let output = self.space.outputs().next().unwrap();
                let output_geo = self.space.output_geometry(output).unwrap();
                let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();

                if let Some(pointer) = self.seat.get_pointer() {
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
    }
}
