pub mod markdown;
pub mod vault;

fn main() {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "rusty-notes",
        native_options,
        Box::new(|_cc| Ok(Box::new(App::default()))),
    )
    .expect("failed to start rusty-notes");
}

#[derive(Default)]
struct App;

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("rusty-notes");
            ui.label("scaffold");
        });
    }
}
