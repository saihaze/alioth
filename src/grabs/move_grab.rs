use smithay::{
    desktop::Window,
    input::{
        pointer::{
            AxisFrame, ButtonEvent, GrabStartData, PointerGrab, PointerInnerHandle,
            RelativeMotionEvent,
        },
        SeatHandler,
    },
    utils::{Logical, Point},
};

use crate::state::State;

pub struct MoveSurfaceGrab<BackendData: 'static> {
    pub start_data: GrabStartData<State<BackendData>>,
    pub window: Window,
    pub initial_location: Point<i32, Logical>,
}

impl<BackendData: 'static> PointerGrab<State<BackendData>> for MoveSurfaceGrab<BackendData> {
    fn motion(
        &mut self,
        data: &mut State<BackendData>,
        handle: &mut PointerInnerHandle<'_, State<BackendData>>,
        focus: Option<(
            <State<BackendData> as SeatHandler>::PointerFocus,
            Point<i32, Logical>,
        )>,
        event: &smithay::input::pointer::MotionEvent,
    ) {
        handle.motion(data, focus, event);

        let delta = event.location - self.start_data.location;
        let new_location = delta + self.initial_location.to_f64();
        data.space
            .map_element(self.window.clone(), new_location.to_i32_round(), true);
    }

    fn relative_motion(
        &mut self,
        data: &mut State<BackendData>,
        handle: &mut PointerInnerHandle<'_, State<BackendData>>,
        focus: Option<(
            <State<BackendData> as SeatHandler>::PointerFocus,
            Point<i32, Logical>,
        )>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn axis(
        &mut self,
        data: &mut State<BackendData>,
        handle: &mut PointerInnerHandle<'_, State<BackendData>>,
        details: AxisFrame,
    ) {
        handle.axis(data, details);
    }

    fn button(
        &mut self,
        data: &mut State<BackendData>,
        handle: &mut smithay::input::pointer::PointerInnerHandle<'_, State<BackendData>>,
        event: &ButtonEvent,
    ) {
        handle.button(data, event);

        if !handle.current_pressed().contains(&0x110) {
            handle.unset_grab(data, event.serial, event.time);
        }
    }

    fn start_data(&self) -> &GrabStartData<State<BackendData>> {
        &self.start_data
    }
}
