use egui::{load::SizedTexture, pos2, Vec2};
use egui_euc::euc_to_egui_colorimage;

const WIDTH: usize = 320;
const HEIGHT: usize = 240;

fn main() -> anyhow::Result<()> {
    eframe::run_native(
        "eframe template",
        Default::default(),
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
    .map_err(|e| anyhow::format_err!("{e}"))
}

struct App {
    tex: egui::TextureId,
    sub: SubGui,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let options = egui::TextureOptions::NEAREST;
        let image = egui::ImageData::from(egui::ColorImage::filled(
            [WIDTH, HEIGHT],
            egui::Color32::RED,
        ));
        let tex = cc
            .egui_ctx
            .tex_manager()
            .write()
            .alloc("sub-gui".into(), image, options);

        Self {
            tex,
            sub: SubGui::new(),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("hi");

            let (resp, painter) = ui.allocate_painter(
                Vec2::new(WIDTH as _, HEIGHT as _),
                egui::Sense::click_and_drag(),
            );
            painter.image(
                self.tex,
                resp.rect,
                egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );

            let raw_input = ctx.input(|r| r.raw.clone());
            let new_image = self.sub.update(raw_input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.strong("STRONG aura");
                });
            });

            ui.ctx().tex_manager().write().set(
                self.tex,
                egui::epaint::ImageDelta::full(new_image, egui::TextureOptions::NEAREST),
            );
        });
    }
}

struct SubGui {
    egui_ctx: egui::Context,
    software_render: egui_euc::Painter,
}

impl SubGui {
    pub fn new() -> Self {
        Self {
            egui_ctx: Default::default(),
            software_render: egui_euc::Painter::new(),
        }
    }

    pub fn update(
        &mut self,
        new_input: egui::RawInput,
        sub_gui: impl FnMut(&egui::Context),
    ) -> egui::ColorImage {
        let output = self.egui_ctx.run(new_input, sub_gui);
        let pixels_per_point = self.egui_ctx.pixels_per_point();
        let clipped_primitives = self.egui_ctx.tessellate(output.shapes, pixels_per_point);
        let buffer = self.software_render.paint_and_update_textures(
            &output.textures_delta,
            &clipped_primitives,
            pixels_per_point,
            [WIDTH, HEIGHT],
        );
        euc_to_egui_colorimage(buffer)
    }
}

/*
use egui::{epaint, Color32, Rgba};
use egui_euc::EguiMeshEucPipeline;
use error_iter::ErrorIter as _;
use euc::rasterizer::Triangles;
use euc::{Buffer2d, CullMode, Pipeline, Sampler, Target, Texture, TriangleList};
use log::error;
use pixels::{Error, Pixels, SurfaceTexture};
use winit::dpi::LogicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::EventLoop;
use winit::keyboard::KeyCode;
use winit::window::WindowBuilder;
use winit_input_helper::WinitInputHelper;

const WIDTH: u32 = 320;
const HEIGHT: u32 = 240;

/// Representation of the application state. In this example, a box will bounce around the screen.
struct World {}

fn main() -> Result<(), Error> {
    env_logger::init();
    let event_loop = EventLoop::new().unwrap();
    let mut input = WinitInputHelper::new();
    let window = {
        let size = LogicalSize::new(WIDTH as f64, HEIGHT as f64);
        WindowBuilder::new()
            .with_title("Hello Pixels")
            .with_inner_size(size)
            .with_min_inner_size(size)
            .build(&event_loop)
            .unwrap()
    };

    let mut pixels = {
        let window_size = window.inner_size();
        let surface_texture = SurfaceTexture::new(window_size.width, window_size.height, &window);
        Pixels::new(WIDTH, HEIGHT, surface_texture)?
    };
    let mut world = World::new();

    let res = event_loop.run(|event, elwt| {
        // Draw the current frame
        if let Event::WindowEvent {
            event: WindowEvent::RedrawRequested,
            ..
        } = event
        {
            world.draw(pixels.frame_mut());
            if let Err(err) = pixels.render() {
                log_error("pixels.render", err);
                elwt.exit();
                return;
            }
        }

        // Handle input events
        if input.update(&event) {
            // Close events
            if input.key_pressed(KeyCode::Escape) || input.close_requested() {
                elwt.exit();
                return;
            }

            // Resize the window
            if let Some(size) = input.window_resized() {
                if let Err(err) = pixels.resize_surface(size.width, size.height) {
                    log_error("pixels.resize_surface", err);
                    elwt.exit();
                    return;
                }
            }

            // Update internal state and request a redraw
            world.update();
            window.request_redraw();
        }
    });
    res.map_err(|e| Error::UserDefined(Box::new(e)))
}

fn log_error<E: std::error::Error + 'static>(method_name: &str, err: E) {
    error!("{method_name}() failed: {err}");
    for source in err.sources().skip(1) {
        error!("  Caused by: {source}");
    }
}

impl World {
    /// Create a new `World` instance that can draw a moving box.
    fn new() -> Self {
        Self {}
    }

    /// Update the `World` internal state; bounce the box around the screen.
    fn update(&mut self) {}

    /// Draw the `World` state to the frame buffer.
    ///
    /// Assumes the default texture format: `wgpu::TextureFormat::Rgba8UnormSrgb`
    fn draw(&self, frame: &mut [u8]) {
        let mut color = Buffer2d::fill([WIDTH as usize, HEIGHT as usize], 0);
        let mut depth = Buffer2d::fill([WIDTH as usize, HEIGHT as usize], 1.0);

        /*
        let circle = epaint::Shape::Circle(epaint::CircleShape::filled(
            egui::Pos2::ZERO,
            30.0,
            egui::Color32::ORANGE,
        ));
        */

        let rect = egui::Rect::from_two_pos(egui::Pos2::ZERO, egui::Pos2::ZERO + egui::Vec2::ONE * 10.0);
        let circle = epaint::Shape::Rect(epaint::RectShape {
            rect,
            corner_radius: egui::CornerRadius::ZERO,
            fill: egui::Color32::ORANGE,
            stroke: egui::Stroke::new(1.0, Color32::RED),
            stroke_kind: egui::StrokeKind::Outside,
            round_to_pixels: None,
            blur_width: 0.0,
            brush: None,
        });

        let mut tess = epaint::tessellator::Tessellator::new(
            1.0,
            epaint::TessellationOptions::default(),
            [100, 100],
            vec![],
        );
        let mut mesh = epaint::Mesh::default();
        tess.tessellate_shape(circle, &mut mesh);

        let texture = Buffer2d::fill([100, 100], [1_f32; 4]);

        let sampler = texture
            .map(|[r, g, b, a]| egui::Rgba::from_rgba_premultiplied(r, g, b, a))
            .linear();

        //let mut scissor = Scissor::new(&mut color, 100, 100, 100, 100);
        let pipeline = EguiMeshEucPipeline {
            vertices: &mesh.vertices,
            sampler,
        };

        // Reverse the indices
        //let mut indices = mesh.indices.clone();
        //let reversed_triangles: Vec<u32> = mesh.indices.chunks_exact(3).map(|f| f.iter().copied().rev()).flatten().collect();
        //indices.extend(reversed_triangles);

        pipeline.render(
            //indices,
            mesh.indices,
            &mut color,
            //&mut scissor,
            &mut depth,
        );

        frame.copy_from_slice(bytemuck::cast_slice(color.raw()));
    }
}

*/
