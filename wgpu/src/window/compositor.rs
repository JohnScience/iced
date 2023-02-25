//! Connect a window with a renderer.
use crate::{Backend, Color, Error, Primitive, Renderer, Settings, Viewport};

use futures::stream::{self, StreamExt};

use iced_graphics::window::compositor;
use iced_native::futures;
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};

use std::marker::PhantomData;

/// A window graphics backend for iced powered by `wgpu`.
#[allow(missing_debug_implementations)]
pub struct Compositor<Theme> {
    settings: Settings,
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    theme: PhantomData<Theme>,
}

impl<Theme> Compositor<Theme> {
    /// Requests a new [`Compositor`] with the given [`Settings`].
    ///
    /// Returns `None` if no compatible graphics adapter could be found.
    pub async fn request<W: HasRawWindowHandle + HasRawDisplayHandle>(
        settings: Settings,
        compatible_window: Option<&W>,
    ) -> Option<Self> {
        let instance = wgpu::Instance::new(settings.internal_backend);

        log::info!("{:#?}", settings);

        #[cfg(not(target_arch = "wasm32"))]
        if log::max_level() >= log::LevelFilter::Info {
            let available_adapters: Vec<_> = instance
                .enumerate_adapters(settings.internal_backend)
                .map(|adapter| adapter.get_info())
                .collect();
            log::info!("Available adapters: {:#?}", available_adapters);
        }

        #[allow(unsafe_code)]
        let compatible_surface = compatible_window
            .map(|window| unsafe { instance.create_surface(window) });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: if settings.antialiasing.is_none() {
                    wgpu::PowerPreference::LowPower
                } else {
                    wgpu::PowerPreference::HighPerformance
                },
                compatible_surface: compatible_surface.as_ref(),
                force_fallback_adapter: false,
            })
            .await?;

        log::info!("Selected: {:#?}", adapter.get_info());

        let format = compatible_surface.as_ref().and_then(|surface| {
            surface.get_supported_formats(&adapter).first().copied()
        })?;

        log::info!("Selected format: {:?}", format);

        #[cfg(target_arch = "wasm32")]
        let limits = [wgpu::Limits::downlevel_webgl2_defaults()
            .using_resolution(adapter.limits())];

        #[cfg(not(target_arch = "wasm32"))]
        let limits =
            [wgpu::Limits::default(), wgpu::Limits::downlevel_defaults()];

        let limits = limits.into_iter().map(|limits| wgpu::Limits {
            max_bind_groups: 2,
            ..limits
        });

        let (device, queue) = stream::iter(limits)
            .filter_map(|limits| async {
                adapter.request_device(
                    &wgpu::DeviceDescriptor {
                        label: Some(
                            "iced_wgpu::window::compositor device descriptor",
                        ),
                        features: wgpu::Features::empty(),
                        limits,
                    },
                    None,
                ).await.ok()
            })
            .boxed()
            .next()
            .await?;

        Some(Compositor {
            instance,
            settings,
            adapter,
            device,
            queue,
            format,
            theme: PhantomData,
        })
    }

    /// Creates a new rendering [`Backend`] for this [`Compositor`].
    pub fn create_backend(&self) -> Backend {
        Backend::new(&self.device, &self.queue, self.settings, self.format)
    }
}

/// Creates a [`Compositor`] and its [`Backend`] for the given [`Settings`] and
/// window.
pub fn new<Theme, W: HasRawWindowHandle + HasRawDisplayHandle>(
    settings: Settings,
    compatible_window: Option<&W>,
) -> Result<(Compositor<Theme>, Backend), Error> {
    let compositor = futures::executor::block_on(Compositor::request(
        settings,
        compatible_window,
    ))
    .ok_or(Error::GraphicsAdapterNotFound)?;

    let backend = compositor.create_backend();

    Ok((compositor, backend))
}

/// Presents the given primitives with the given [`Compositor`] and [`Backend`].
pub fn present<Theme, T: AsRef<str>>(
    compositor: &mut Compositor<Theme>,
    backend: &mut Backend,
    surface: &mut wgpu::Surface,
    primitives: &[Primitive],
    viewport: &Viewport,
    background_color: Color,
    overlay: &[T],
) -> Result<(), compositor::SurfaceError> {
    match surface.get_current_texture() {
        Ok(frame) => {
            let mut encoder = compositor.device.create_command_encoder(
                &wgpu::CommandEncoderDescriptor {
                    label: Some("iced_wgpu encoder"),
                },
            );

            let view = &frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());

            backend.present(
                &compositor.device,
                &compositor.queue,
                &mut encoder,
                Some(background_color),
                view,
                primitives,
                viewport,
                overlay,
            );

            // Submit work
            let _submission = compositor.queue.submit(Some(encoder.finish()));
            frame.present();

            Ok(())
        }
        Err(error) => match error {
            wgpu::SurfaceError::Timeout => {
                Err(compositor::SurfaceError::Timeout)
            }
            wgpu::SurfaceError::Outdated => {
                Err(compositor::SurfaceError::Outdated)
            }
            wgpu::SurfaceError::Lost => Err(compositor::SurfaceError::Lost),
            wgpu::SurfaceError::OutOfMemory => {
                Err(compositor::SurfaceError::OutOfMemory)
            }
        },
    }
}

impl<Theme> iced_graphics::window::Compositor for Compositor<Theme> {
    type Settings = Settings;
    type Renderer = Renderer<Theme>;
    type Surface = wgpu::Surface;

    fn new<W: HasRawWindowHandle + HasRawDisplayHandle>(
        settings: Self::Settings,
        compatible_window: Option<&W>,
    ) -> Result<(Self, Self::Renderer), Error> {
        let (compositor, backend) = new(settings, compatible_window)?;

        Ok((compositor, Renderer::new(backend)))
    }

    fn create_surface<W: HasRawWindowHandle + HasRawDisplayHandle>(
        &mut self,
        window: &W,
        width: u32,
        height: u32,
    ) -> wgpu::Surface {
        #[allow(unsafe_code)]
        let mut surface = unsafe { self.instance.create_surface(window) };

        self.configure_surface(&mut surface, width, height);

        surface
    }

    fn configure_surface(
        &mut self,
        surface: &mut Self::Surface,
        width: u32,
        height: u32,
    ) {
        surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: self.format,
                present_mode: self.settings.present_mode,
                width,
                height,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
            },
        );
    }

    fn fetch_information(&self) -> compositor::Information {
        let information = self.adapter.get_info();

        compositor::Information {
            adapter: information.name,
            backend: format!("{:?}", information.backend),
        }
    }

    fn present<T: AsRef<str>>(
        &mut self,
        renderer: &mut Self::Renderer,
        surface: &mut Self::Surface,
        viewport: &Viewport,
        background_color: Color,
        overlay: &[T],
    ) -> Result<(), compositor::SurfaceError> {
        renderer.with_primitives(|backend, primitives| {
            present(
                self,
                backend,
                surface,
                primitives,
                viewport,
                background_color,
                overlay,
            )
        })
    }
}
