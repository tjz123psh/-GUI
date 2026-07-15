use super::StageUi;
use adw::prelude::*;
use gtk4 as gtk;
use libadwaita as adw;

pub(super) fn icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .css_classes(["flat"])
        .build()
}

pub(super) fn text_action_button(icon: &str, label: &str, classes: &[&str]) -> gtk::Button {
    let button = gtk::Button::builder()
        .hexpand(true)
        .css_classes(classes)
        .build();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::Center)
        .build();
    content.append(&gtk::Image::from_icon_name(icon));
    content.append(&gtk::Label::new(Some(label)));
    button.set_child(Some(&content));
    button
}

pub(super) fn quick_action_button(icon: &str, title: &str, subtitle: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .hexpand(true)
        .css_classes(["quick-action"])
        .build();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .build();
    let icon_wrap = gtk::Box::builder()
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .css_classes(["quick-action-icon"])
        .build();
    icon_wrap.append(&gtk::Image::from_icon_name(icon));
    content.append(&icon_wrap);
    let copy = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .hexpand(true)
        .build();
    copy.append(
        &gtk::Label::builder()
            .label(title)
            .halign(gtk::Align::Start)
            .css_classes(["quick-action-title"])
            .build(),
    );
    copy.append(
        &gtk::Label::builder()
            .label(subtitle)
            .halign(gtk::Align::Start)
            .wrap(true)
            .xalign(0.0)
            .css_classes(["quick-action-subtitle"])
            .build(),
    );
    content.append(&copy);
    content.append(&gtk::Image::from_icon_name("go-next-symbolic"));
    button.set_child(Some(&content));
    button
}

pub(super) fn stage_widget(icon: &str, title: &str) -> (gtk::Box, StageUi) {
    let item = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .css_classes(["stage-item"])
        .build();
    let dot = gtk::Box::builder()
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .css_classes(["stage-dot", "stage-idle"])
        .build();
    dot.append(&gtk::Image::from_icon_name(icon));
    item.append(&dot);
    let copy = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(1)
        .build();
    copy.append(
        &gtk::Label::builder()
            .label(title)
            .halign(gtk::Align::Start)
            .css_classes(["stage-title"])
            .build(),
    );
    let value = gtk::Label::builder()
        .label("检查中")
        .halign(gtk::Align::Start)
        .css_classes(["stage-value"])
        .build();
    copy.append(&value);
    item.append(&copy);
    (item, StageUi { dot, value })
}

pub(super) fn section_heading(title: &str, description: &str) -> gtk::Box {
    let copy = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(3)
        .build();
    copy.append(
        &gtk::Label::builder()
            .label(title)
            .halign(gtk::Align::Start)
            .css_classes(["section-title"])
            .build(),
    );
    copy.append(
        &gtk::Label::builder()
            .label(description)
            .halign(gtk::Align::Start)
            .wrap(true)
            .xalign(0.0)
            .css_classes(["section-description"])
            .build(),
    );
    copy
}

pub(super) fn status_row(icon: &str, title: &str) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle("检查中")
        .build();
    row.add_prefix(&gtk::Image::from_icon_name(icon));
    row
}

pub(super) fn log_view(buffer: &gtk::TextBuffer) -> gtk::TextView {
    gtk::TextView::builder()
        .buffer(buffer)
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .left_margin(14)
        .right_margin(14)
        .top_margin(12)
        .bottom_margin(12)
        .css_classes(["log-view"])
        .build()
}
