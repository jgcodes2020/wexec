use std::num::NonZeroUsize;

use vello::{
    kurbo::{Affine, Circle, Ellipse, Line, RoundedRect, Stroke}, peniko::Color, util::{RenderContext, RenderSurface}, wgpu, AaConfig, Renderer, RendererOptions, Scene
};
use wexec::Runtime;
use winit::{dpi::LogicalSize, event::WindowEvent, window::Window};

fn main() {
    let rt = Runtime::new();

    rt.start_with(main2())
}

async fn main2() {
    let mut context = RenderContext::new();
    let mut scene = Scene::new();
    let mut renderers = Vec::<Option<Renderer>>::new();

    wexec::resumed().await;

    let window = create_winit_window();
    let size = window.inner_size();
    let mut surface = context
        .create_surface(
            &window,
            size.width,
            size.height,
            wgpu::PresentMode::AutoVsync,
        )
        .await
        .unwrap();

    renderers.resize_with(context.devices.len(), || None);
    renderers[surface.dev_id].get_or_insert_with(|| create_vello_renderer(&context, &surface));

    loop {
        match wexec::window_event(window.id()).await {
            // Exit the event loop when a close is requested (e.g. window's close button is pressed)
            WindowEvent::CloseRequested => return,

            // Resize the surface when the window is resized
            WindowEvent::Resized(size) => {
                context
                    .resize_surface(&mut surface, size.width, size.height);
            }

            // This is where all the rendering happens
            WindowEvent::RedrawRequested => {
                // Empty the scene of objects to draw. You could create a new Scene each time, but in this case
                // the same Scene is reused so that the underlying memory allocation can also be reused.
                scene.reset();

                // Re-add the objects to draw to the scene.
                add_shapes_to_scene(&mut scene);

                // Get the window size
                let width = surface.config.width;
                let height = surface.config.height;

                // Get a handle to the device
                let device_handle = &context.devices[surface.dev_id];

                // Get the surface's texture
                let surface_texture = surface
                    .surface
                    .get_current_texture()
                    .expect("failed to get surface texture");

                // Render to the surface's texture
                renderers[surface.dev_id]
                    .as_mut()
                    .unwrap()
                    .render_to_surface(
                        &device_handle.device,
                        &device_handle.queue,
                        &scene,
                        &surface_texture,
                        &vello::RenderParams {
                            base_color: vello::peniko::Color::BLACK, // Background color
                            width,
                            height,
                            antialiasing_method: AaConfig::Msaa16,
                        },
                    )
                    .expect("failed to render to surface");

                // Queue the texture to be presented on the surface
                surface_texture.present();

                device_handle.device.poll(wgpu::Maintain::Poll);
            }
            _ => {}
        }
    }
}

/// Helper function that creates a Winit window and returns it (wrapped in an Arc for sharing between threads)
fn create_winit_window() -> Window {
    let attr = Window::default_attributes()
        .with_inner_size(LogicalSize::new(1044, 800))
        .with_resizable(true)
        .with_title("Vello Shapes");
    wexec::create_window(attr).unwrap()
}

/// Helper function that creates a vello `Renderer` for a given `RenderContext` and `RenderSurface`
fn create_vello_renderer(render_cx: &RenderContext, surface: &RenderSurface<'_>) -> Renderer {
    Renderer::new(
        &render_cx.devices[surface.dev_id].device,
        RendererOptions {
            surface_format: Some(surface.format),
            use_cpu: false,
            antialiasing_support: vello::AaSupport::all(),
            num_init_threads: NonZeroUsize::new(1),
        },
    )
    .expect("Couldn't create renderer")
}

/// Add shapes to a vello scene. This does not actually render the shapes, but adds them
/// to the Scene data structure which represents a set of objects to draw.
fn add_shapes_to_scene(scene: &mut Scene) {
    // Draw an outlined rectangle
    let stroke = Stroke::new(6.0);
    let rect = RoundedRect::new(10.0, 10.0, 240.0, 240.0, 20.0);
    let rect_stroke_color = Color::rgba(0.9804, 0.702, 0.5294, 1.);
    scene.stroke(&stroke, Affine::IDENTITY, rect_stroke_color, None, &rect);

    // Draw a filled circle
    let circle = Circle::new((420.0, 200.0), 120.0);
    let circle_fill_color = Color::rgba(0.9529, 0.5451, 0.6588, 1.);
    scene.fill(
        vello::peniko::Fill::NonZero,
        Affine::IDENTITY,
        circle_fill_color,
        None,
        &circle,
    );

    // Draw a filled ellipse
    let ellipse = Ellipse::new((250.0, 420.0), (100.0, 160.0), -90.0);
    let ellipse_fill_color = Color::rgba(0.7961, 0.651, 0.9686, 1.);
    scene.fill(
        vello::peniko::Fill::NonZero,
        Affine::IDENTITY,
        ellipse_fill_color,
        None,
        &ellipse,
    );

    // Draw a straight line
    let line = Line::new((260.0, 20.0), (620.0, 100.0));
    let line_stroke_color = Color::rgba(0.5373, 0.7059, 0.9804, 1.);
    scene.stroke(&stroke, Affine::IDENTITY, line_stroke_color, None, &line);
}
