use egui::Rgba;
use error_iter::ErrorIter as _;
use euc::rasterizer::Triangles;
use euc::{Buffer2d, Pipeline, TriangleList};
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
const BOX_SIZE: i16 = 64;

/// Representation of the application state. In this example, a box will bounce around the screen.
struct World {
    box_x: i16,
    box_y: i16,
    velocity_x: i16,
    velocity_y: i16,
}

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
        Self {
            box_x: 24,
            box_y: 16,
            velocity_x: 1,
            velocity_y: 1,
        }
    }

    /// Update the `World` internal state; bounce the box around the screen.
    fn update(&mut self) {
        if self.box_x <= 0 || self.box_x + BOX_SIZE > WIDTH as i16 {
            self.velocity_x *= -1;
        }
        if self.box_y <= 0 || self.box_y + BOX_SIZE > HEIGHT as i16 {
            self.velocity_y *= -1;
        }

        self.box_x += self.velocity_x;
        self.box_y += self.velocity_y;
    }

    /// Draw the `World` state to the frame buffer.
    ///
    /// Assumes the default texture format: `wgpu::TextureFormat::Rgba8UnormSrgb`
    fn draw(&self, frame: &mut [u8]) {
        let mut color = Buffer2d::fill([WIDTH as usize, HEIGHT as usize], 0);
        let mut depth = Buffer2d::fill([WIDTH as usize, HEIGHT as usize], 1.0);

        Example.render(
            vec![
            [-1.0, -1.0, 0.0],
            [ 1.0, -1.0, 0.0],
            [ 0.0,  1.0, 0.0],
            ],
            &mut color,
            &mut depth,
        );

        frame.copy_from_slice(bytemuck::cast_slice(color.raw()));
    }
}

/*
   struct EucBorrowedFrameBuffer<'a> {
   pixels: &'a mut [u32],
   width: usize,
   height: usize,
   }

   impl<'a> EucBorrowedFrameBuffer<'a> {
   pub fn new(buf: &'a mut [u8], width: usize, height: usize) -> Self {
   Self {
   pixels: bytemuck::cast_slice_mut(buf),
   height,
   width,
   }
   }

   fn index(&self, x: usize, y: usize) -> usize {
   x + y * self.width
   }
   }

   impl euc::Texture<2> for EucBorrowedFrameBuffer<'_> {
   type Index = usize;
   type Texel = u32;

   fn size(&self) -> [Self::Index; 2] {
   [self.width, self.height]
   }

   fn read(&self, [x, y]: [Self::Index; 2]) -> Self::Texel {
   self.pixels[self.index(x, y)]
   }
   }

   impl euc::Target for EucBorrowedFrameBuffer<'_> {
   unsafe fn read_exclusive_unchecked(&self, x: usize, y: usize) -> Self::Texel {
   use euc::Texture;
   self.read([x, y])
   }

   unsafe fn write_exclusive_unchecked(&self, x: usize, y: usize, texel: Self::Texel) {
   let idx = self.index(x, y);
   self.pixels[idx] = texel;
   }
   }
   */

struct Example;


impl<'r> Pipeline<'r> for Example {
    type Vertex = [f32; 3];
    type VertexData = Rgba;
    type Primitives = TriangleList;
    type Pixel = u32;
    type Fragment = Rgba;

    #[inline(always)]
    fn vertex(&self, [x, y, z]: &Self::Vertex) -> ([f32; 4], Self::VertexData) {
        ([*x, *y, *z, 1.0], Rgba::from_rgb(*x, *y, *z))
    }

    #[inline(always)]
    fn fragment(&self, color: Self::VertexData) -> Self::Fragment {
        color
    }

    fn blend(&self, _: Self::Pixel, color: Self::Fragment) -> Self::Pixel {
        u32::from_le_bytes(color.to_srgba_unmultiplied())
    }
}


