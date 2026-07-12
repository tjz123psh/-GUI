mod config;
mod system;
mod ui;

use adw::prelude::*;
use gtk::glib;
use gtk4 as gtk;
use libadwaita as adw;

fn main() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id(config::APP_ID)
        .build();

    app.connect_startup(|_| {
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
        ui::install_css();
    });
    app.connect_activate(ui::build);
    app.run()
}
