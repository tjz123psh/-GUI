use super::navigation::SidebarUi;
use adw::prelude::*;
use gtk4 as gtk;
use libadwaita as adw;

#[allow(clippy::too_many_arguments)]
pub(super) fn install_breakpoints(
    window: &adw::ApplicationWindow,
    connection_clamp: &adw::Clamp,
    sidebar: &SidebarUi,
    columns: &gtk::Box,
    link_top: &gtk::Box,
    action_wrap: &gtk::Box,
    action_btn: &gtk::Button,
    header_switcher: &adw::ViewSwitcher,
    bottom_switcher: &adw::ViewSwitcherBar,
    connection_content: &gtk::Box,
    diagnostics_page: &gtk::Box,
) {
    let standard_condition = adw::BreakpointCondition::new_and(
        adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MinWidth,
            720.0,
            adw::LengthUnit::Sp,
        ),
        adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            1099.0,
            adw::LengthUnit::Sp,
        ),
    );
    let standard = adw::Breakpoint::new(standard_condition);
    standard.add_setter(&sidebar.root, "visible", Some(&false.to_value()));
    standard.add_setter(&sidebar.compact_root, "visible", Some(&true.to_value()));
    standard.add_setter(
        columns,
        "orientation",
        Some(&gtk::Orientation::Vertical.to_value()),
    );
    standard.add_setter(
        link_top,
        "orientation",
        Some(&gtk::Orientation::Vertical.to_value()),
    );
    standard.add_setter(action_wrap, "width-request", Some(&(-1_i32).to_value()));
    standard.add_setter(action_wrap, "halign", Some(&gtk::Align::Fill.to_value()));
    standard.add_setter(action_btn, "hexpand", Some(&true.to_value()));
    standard.add_setter(connection_clamp, "maximum-size", Some(&820_i32.to_value()));
    window.add_breakpoint(standard);

    let expanded_condition = adw::BreakpointCondition::new_and(
        adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MinWidth,
            1100.0,
            adw::LengthUnit::Sp,
        ),
        adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            1399.0,
            adw::LengthUnit::Sp,
        ),
    );
    let expanded = adw::Breakpoint::new(expanded_condition);
    expanded.add_setter(&sidebar.root, "visible", Some(&false.to_value()));
    expanded.add_setter(&sidebar.compact_root, "visible", Some(&true.to_value()));
    window.add_breakpoint(expanded);

    let compact = adw::Breakpoint::new(
        adw::BreakpointCondition::parse("max-width: 719sp").expect("valid breakpoint"),
    );
    compact.add_setter(
        link_top,
        "orientation",
        Some(&gtk::Orientation::Vertical.to_value()),
    );
    compact.add_setter(&sidebar.root, "visible", Some(&false.to_value()));
    compact.add_setter(&sidebar.compact_root, "visible", Some(&false.to_value()));
    compact.add_setter(
        columns,
        "orientation",
        Some(&gtk::Orientation::Vertical.to_value()),
    );
    compact.add_setter(connection_clamp, "maximum-size", Some(&820_i32.to_value()));
    compact.add_setter(action_wrap, "width-request", Some(&(-1_i32).to_value()));
    compact.add_setter(action_wrap, "halign", Some(&gtk::Align::Fill.to_value()));
    compact.add_setter(action_btn, "hexpand", Some(&true.to_value()));
    compact.add_setter(header_switcher, "visible", Some(&false.to_value()));
    compact.add_setter(bottom_switcher, "reveal", Some(&true.to_value()));
    for widget in [connection_content, diagnostics_page] {
        compact.add_setter(widget, "margin-start", Some(&16_i32.to_value()));
        compact.add_setter(widget, "margin-end", Some(&16_i32.to_value()));
        compact.add_setter(widget, "margin-top", Some(&16_i32.to_value()));
        compact.add_setter(widget, "margin-bottom", Some(&16_i32.to_value()));
    }
    window.add_breakpoint(compact);
}
