mod config;
mod scene;
mod system;
mod ui;

use gtk4::gio::prelude::*;
use gtk4::glib;
use libadwaita as adw;

fn main() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id(config::APP_ID)
        .build();
    app.connect_activate(ui::activate);
    app.run()
}
