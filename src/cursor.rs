use std::{collections::BTreeMap, fs::File, io::Read, ops::Bound, time::Duration};

use drm_fourcc::DrmFourcc;
use smithay::{
    backend::renderer::{
        element::{
            surface::{render_elements_from_surface_tree, WaylandSurfaceRenderElement},
            texture::{TextureBuffer, TextureRenderElement},
            AsRenderElements,
        },
        ImportAll, ImportMem, Renderer, Texture,
    },
    input::pointer::CursorImageStatus,
    render_elements,
    utils::{Clock, Monotonic, Transform},
};
use xcursor::{
    parser::{parse_xcursor, Image},
    CursorTheme,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to load cursor icon")]
    CursorIconLoadFailure,
}

pub struct CursorElement {
    default: BTreeMap<u64, Image>,
    total_delay: u64,
    current_delay: u64,
    status: CursorImageStatus,
    size: u32,
}

impl CursorElement {
    pub fn new() -> Result<Self, Error> {
        // Get the xcursor theme.
        let theme = std::env::var("XCURSOR_THEME").unwrap_or("default".into());

        // Get the cursor size. 24px on default.
        let size = std::env::var("XCURSOR_SIZE")
            .ok()
            .and_then(|x| x.parse::<u32>().ok())
            .unwrap_or(24);

        let cursor_theme = CursorTheme::load(&theme);
        let cursor_path = match cursor_theme.load_icon("left_ptr") {
            Some(path) => path,
            None => return Err(Error::CursorIconLoadFailure),
        };

        let mut cursor_file = File::open(&cursor_path).map_err(|_| Error::CursorIconLoadFailure)?;
        let mut cursor_data = Vec::new();
        cursor_file
            .read_to_end(&mut cursor_data)
            .map_err(|_| Error::CursorIconLoadFailure)?;

        let cursor_images = match parse_xcursor(&cursor_data) {
            Some(images) => images
                .into_iter()
                .filter(|image| image.width == size && image.height == size),
            None => return Err(Error::CursorIconLoadFailure),
        };

        let mut default = BTreeMap::new();

        let mut total_delay = 0;
        for image in cursor_images {
            total_delay += image.delay as u64;
            default.insert(total_delay, image);
        }

        Ok(Self {
            default,
            total_delay,
            current_delay: 0,
            status: CursorImageStatus::Default,
            size,
        })
    }

    pub fn update_animation_status(&mut self, clock: &Clock<Monotonic>) {
        let current_duration = Duration::from(clock.now());
        self.current_delay = self.total_delay % current_duration.as_millis() as u64;
    }

    pub fn set_status(&mut self, status: CursorImageStatus) {
        self.status = status;
    }
}

render_elements! {
    pub PointerRenderElement<R> where R: ImportAll;
    Surface = WaylandSurfaceRenderElement<R>,
    Texture = TextureRenderElement<<R as Renderer>::TextureId>,
}

impl<T: Texture + Clone + 'static, R> AsRenderElements<R> for CursorElement
where
    R: Renderer<TextureId = T> + ImportAll + ImportMem,
{
    type RenderElement = PointerRenderElement<R>;

    fn render_elements<E: From<Self::RenderElement>>(
        &self,
        renderer: &mut R,
        location: smithay::utils::Point<i32, smithay::utils::Physical>,
        scale: smithay::utils::Scale<f64>,
        alpha: f32,
    ) -> Vec<E> {
        match &self.status {
            CursorImageStatus::Hidden => vec![],
            CursorImageStatus::Default => {
                let image = self
                    .default
                    .range((Bound::Included(self.current_delay), Bound::Unbounded))
                    .next()
                    .unwrap()
                    .1;

                let texture = renderer
                    .import_memory(
                        image.pixels_rgba.as_slice(),
                        DrmFourcc::Abgr8888,
                        (self.size as i32, self.size as i32).into(),
                        false,
                    )
                    .unwrap();

                let buffer =
                    TextureBuffer::from_texture(renderer, texture, 1, Transform::Normal, None);

                let element =
                    PointerRenderElement::<R>::from(TextureRenderElement::from_texture_buffer(
                        location.to_f64(),
                        &buffer,
                        None,
                        None,
                        None,
                    ))
                    .into();

                vec![element]
            }
            CursorImageStatus::Surface(surface) => {
                render_elements_from_surface_tree(renderer, surface, location, scale, alpha)
                    .into_iter()
                    .map(E::from)
                    .collect()
            }
        }
    }
}
