use smithay::{
    backend::input::{AbsolutePositionEvent, Event, InputBackend, InputEvent, KeyboardKeyEvent},
    input::{keyboard::FilterResult, pointer::MotionEvent},
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
            _ => (),
        }
    }
}
