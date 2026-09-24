pub mod app;
pub mod console;
pub mod device;
pub mod log_parse;
pub mod motec;
pub mod tabs;

pub mod ui {
    slint::include_modules!();
}

use slint::ComponentHandle;

fn main() {
    env_logger::init();

    let window = ui::AppWindow::new().expect("failed to create window");
    let _app = app::App::new(&window);
    window.run().expect("failed to run event loop");
}
