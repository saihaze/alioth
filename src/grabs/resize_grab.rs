use std::cell::RefCell;

use smithay::{
    desktop::{Space, Window},
    input::{
        pointer::{
            AxisFrame, ButtonEvent, GrabStartData, MotionEvent, PointerGrab, PointerInnerHandle,
            RelativeMotionEvent,
        },
        SeatHandler,
    },
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::protocol::wl_surface::WlSurface,
    },
    utils::{Logical, Point, Rectangle, Size},
    wayland::{compositor, shell::xdg::SurfaceCachedState},
};

use crate::state::State;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct ResizeEdge: u32 {
        const TOP          = 0b0001;
        const BOTTOM       = 0b0010;
        const LEFT         = 0b0100;
        const RIGHT        = 0b1000;

        const TOP_LEFT     = Self::TOP.bits() | Self::LEFT.bits();
        const BOTTOM_LEFT  = Self::BOTTOM.bits() | Self::LEFT.bits();

        const TOP_RIGHT    = Self::TOP.bits() | Self::RIGHT.bits();
        const BOTTOM_RIGHT = Self::BOTTOM.bits() | Self::RIGHT.bits();
    }
}

impl From<xdg_toplevel::ResizeEdge> for ResizeEdge {
    fn from(value: xdg_toplevel::ResizeEdge) -> Self {
        Self::from_bits(value as u32).unwrap()
    }
}

impl<BackenData> PointerGrab<State<BackenData>> for ResizeSurfaceGrab<BackenData> {
    fn motion(
        &mut self,
        data: &mut State<BackenData>,
        handle: &mut PointerInnerHandle<'_, State<BackenData>>,
        focus: Option<(
            <State<BackenData> as SeatHandler>::PointerFocus,
            Point<i32, Logical>,
        )>,
        event: &MotionEvent,
    ) {
        handle.motion(data, focus, event);

        let mut delta = event.location - self.start_data.location;

        let mut new_window_width = self.initial_geo.size.w;
        let mut new_window_height = self.initial_geo.size.h;

        if self.edges.intersects(ResizeEdge::LEFT | ResizeEdge::RIGHT) {
            if self.edges.intersects(ResizeEdge::LEFT) {
                delta.x = -delta.x;
            }

            new_window_width = (self.initial_geo.size.w as f64 + delta.x) as i32;
        }

        if self.edges.intersects(ResizeEdge::TOP | ResizeEdge::BOTTOM) {
            if self.edges.intersects(ResizeEdge::TOP) {
                delta.y = -delta.y;
            }

            new_window_height = (self.initial_geo.size.h as f64 + delta.y) as i32;
        }

        let (min_size, max_size) =
            compositor::with_states(self.window.toplevel().wl_surface(), |states| {
                let data = states.cached_state.current::<SurfaceCachedState>();
                (data.min_size, data.max_size)
            });

        let min_width = min_size.w.max(1);
        let min_height = min_size.h.max(1);

        let max_width = if max_size.w == 0 {
            i32::MAX
        } else {
            max_size.w
        };

        let max_height = if max_size.h == 0 {
            i32::MAX
        } else {
            max_size.h
        };

        self.last_window_size = Size::from((
            new_window_width.max(min_width).min(max_width),
            new_window_height.max(min_height).min(max_height),
        ));

        let toplevel = self.window.toplevel();
        toplevel.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Resizing);
            state.size = Some(self.last_window_size);
        });
        toplevel.send_pending_configure();
    }

    fn relative_motion(
        &mut self,
        data: &mut State<BackenData>,
        handle: &mut PointerInnerHandle<'_, State<BackenData>>,
        focus: Option<(
            <State<BackenData> as SeatHandler>::PointerFocus,
            Point<i32, Logical>,
        )>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn button(
        &mut self,
        data: &mut State<BackenData>,
        handle: &mut PointerInnerHandle<'_, State<BackenData>>,
        event: &ButtonEvent,
    ) {
        handle.button(data, event);

        if !handle.current_pressed().contains(&0x110) {
            handle.unset_grab(data, event.serial, event.time);

            let toplevel = self.window.toplevel();
            toplevel.with_pending_state(|state| {
                state.states.unset(xdg_toplevel::State::Resizing);
                state.size = Some(self.last_window_size);
            });
            toplevel.send_pending_configure();

            ResizeSurfaceState::with(toplevel.wl_surface(), |state| {
                *state = ResizeSurfaceState::Finishing {
                    edges: self.edges,
                    initial_geo: self.initial_geo,
                };
            });
        }
    }

    fn axis(
        &mut self,
        data: &mut State<BackenData>,
        handle: &mut PointerInnerHandle<'_, State<BackenData>>,
        details: AxisFrame,
    ) {
        handle.axis(data, details);
    }

    fn start_data(&self) -> &GrabStartData<State<BackenData>> {
        &self.start_data
    }
}

pub struct ResizeSurfaceGrab<BackendData: 'static> {
    start_data: GrabStartData<State<BackendData>>,
    window: Window,

    edges: ResizeEdge,

    initial_geo: Rectangle<i32, Logical>,
    last_window_size: Size<i32, Logical>,
}

impl<BackendData: 'static> ResizeSurfaceGrab<BackendData> {
    pub fn start(
        start_data: GrabStartData<State<BackendData>>,
        window: Window,
        edges: ResizeEdge,
        initial_window_rect: Rectangle<i32, Logical>,
    ) -> Self {
        let initial_geo = initial_window_rect;

        ResizeSurfaceState::with(window.toplevel().wl_surface(), |state| {
            *state = ResizeSurfaceState::Resizing { edges, initial_geo };
        });

        Self {
            start_data,
            window,
            edges,
            initial_geo,
            last_window_size: initial_geo.size,
        }
    }
}

#[derive(Default)]
enum ResizeSurfaceState {
    #[default]
    Idle,
    Resizing {
        edges: ResizeEdge,
        initial_geo: Rectangle<i32, Logical>,
    },
    // Aka `WaitingForLastCommit` in Smallvil.
    Finishing {
        edges: ResizeEdge,
        initial_geo: Rectangle<i32, Logical>,
    },
}

impl ResizeSurfaceState {
    fn with<F, T>(surface: &WlSurface, cb: F) -> T
    where
        F: FnOnce(&mut Self) -> T,
    {
        compositor::with_states(surface, |states| {
            states.data_map.insert_if_missing(RefCell::<Self>::default);
            let state = states.data_map.get::<RefCell<Self>>().unwrap();

            cb(&mut state.borrow_mut())
        })
    }

    fn commit(&mut self) -> Option<(ResizeEdge, Rectangle<i32, Logical>)> {
        match *self {
            Self::Resizing { edges, initial_geo } => Some((edges, initial_geo)),
            Self::Finishing { edges, initial_geo } => {
                *self = Self::Idle;
                Some((edges, initial_geo))
            }
            Self::Idle => None,
        }
    }
}

pub fn handle_commit(space: &mut Space<Window>, surface: &WlSurface) -> Option<()> {
    let window = space
        .elements()
        .find(|w| w.toplevel().wl_surface() == surface)
        .cloned()?;

    let mut window_loc = space.element_location(&window)?;
    let geometry = window.geometry();

    let new_loc: Point<Option<i32>, Logical> = ResizeSurfaceState::with(surface, |state| {
        state
            .commit()
            .and_then(|(edges, initial_geo)| {
                edges.intersects(ResizeEdge::TOP_LEFT).then(|| {
                    let new_x = edges
                        .intersects(ResizeEdge::LEFT)
                        .then_some(initial_geo.loc.x + (initial_geo.size.w - geometry.size.w));

                    let new_y = edges
                        .intersects(ResizeEdge::TOP)
                        .then_some(initial_geo.loc.y + (initial_geo.size.h - geometry.size.h));

                    (new_x, new_y).into()
                })
            })
            .unwrap_or_default()
    });

    if let Some(new_x) = new_loc.x {
        window_loc.x = new_x;
    }

    if let Some(new_y) = new_loc.y {
        window_loc.y = new_y;
    }

    if new_loc.x.is_some() || new_loc.y.is_some() {
        space.map_element(window, window_loc, false);
    }

    Some(())
}
