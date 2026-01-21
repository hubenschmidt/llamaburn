use eframe::egui::{self, Ui};

/// Render a multi-select dropdown with "All" / "Clear" buttons.
/// Returns true if the selection changed.
pub fn multi_select_dropdown<T, L>(
    ui: &mut Ui,
    id: &str,
    label_prefix: &str,
    items: &[T],
    selected: &mut Vec<T>,
    label_fn: L,
    interactive: bool,
    min_width: f32,
) -> bool
where
    T: Clone + PartialEq,
    L: Fn(&T) -> String,
{
    let label = format_selection_label(label_prefix, selected.len(), items.len());
    let popup_id = ui.make_persistent_id(id);
    let btn = ui.add_enabled(
        interactive,
        egui::Button::new(&label).min_size(egui::vec2(290.0, 0.0)),
    );

    let mut changed = false;

    if btn.clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(popup_id));
    }

    egui::popup_below_widget(
        ui,
        popup_id,
        &btn,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            ui.set_min_width(min_width);
            ui.horizontal(|ui| {
                if ui.small_button("All").clicked() {
                    *selected = items.to_vec();
                    changed = true;
                }
                if ui.small_button("Clear").clicked() {
                    selected.clear();
                    changed = true;
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                for item in items {
                    let mut is_selected = selected.contains(item);
                    if ui.checkbox(&mut is_selected, label_fn(item)).changed() {
                        toggle_selection(selected, item.clone(), is_selected);
                        changed = true;
                    }
                }
            });
        },
    );

    changed
}

/// Toggle an item in/out of a selection vec
pub fn toggle_selection<T: Clone + PartialEq>(vec: &mut Vec<T>, item: T, selected: bool) {
    if selected && !vec.contains(&item) {
        vec.push(item);
        return;
    }
    if !selected {
        vec.retain(|x| x != &item);
    }
}

/// Format dropdown label showing selection count
pub fn format_selection_label(name: &str, selected: usize, total: usize) -> String {
    match selected {
        0 => format!("{}: None", name),
        n if n == total => format!("{}: All ({})", name, total),
        1 => format!("{}: 1 selected", name),
        n => format!("{}: {} selected", name, n),
    }
}
