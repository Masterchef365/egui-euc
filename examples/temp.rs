use egui::{pos2, Vec2};
use egui_euc::SoftwareGui;

const WIDTH: usize = 1640;
const HEIGHT: usize = 1480;

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
    sub: SoftwareGui,
    demo: egui_demo_lib::DemoWindows,
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
            demo: Default::default(),
            tex,
            sub: SoftwareGui::new(),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("hi");

            let (rect, _) = ui.allocate_exact_size(
                Vec2::new(WIDTH as _, HEIGHT as _) / ui.pixels_per_point(),
                egui::Sense::click_and_drag(),
            );

            ui.painter().image(
                self.tex,
                rect,
                egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );

            ui.painter().rect_stroke(rect, 0.0, egui::Stroke::new(1.0, egui::Color32::WHITE), egui::StrokeKind::Inside);

            let mut raw_input = ctx.input(|r| r.raw.clone());

            raw_input.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(WIDTH as f32, HEIGHT as f32) / ui.pixels_per_point()));

            for event in &mut raw_input.events {
                match event {
                    egui::Event::PointerMoved(pos) => {
                        *pos -= rect.min.to_vec2();
                    },
                    egui::Event::PointerButton { pos, .. } => {
                        *pos -= rect.min.to_vec2();
                    }
                    _ => (),
                }
            }

            //self.sub.egui_ctx.set_pixels_per_point(ui.pixels_per_point());
            let new_image = self.sub.update(raw_input, [WIDTH, HEIGHT], |ctx| {
                self.demo.ui(ctx);
            });

            ui.ctx().tex_manager().write().set(
                self.tex,
                egui::epaint::ImageDelta::full(new_image, egui::TextureOptions::NEAREST),
            );
        });
    }
}
