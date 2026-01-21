use eframe::egui::{self, Ui};

/// Render a rankings widget with model best, all-time best, and leaderboard.
pub fn rankings_widget<T>(
    ui: &mut Ui,
    model_best: Option<&T>,
    all_time_best: Option<(&str, &T)>,
    leaderboard: &[(String, T)],
    format_value: impl Fn(&T) -> String,
) {
    let best_label = model_best
        .map(|v| format_value(v))
        .unwrap_or_else(|| "—".to_string());
    ui.label(format!("Model Best: {}", best_label));

    let all_time_label = all_time_best
        .map(|(model, v)| format!("{} ({})", format_value(v), model))
        .unwrap_or_else(|| "—".to_string());
    ui.label(format!("All-Time: {}", all_time_label));

    if leaderboard.is_empty() {
        return;
    }

    ui.add_space(10.0);
    ui.label(
        egui::RichText::new("Leaderboard")
            .small()
            .color(egui::Color32::GRAY),
    );

    for (i, (model, value)) in leaderboard.iter().enumerate() {
        ui.label(format!("{}. {} ({})", i + 1, model, format_value(value)));
    }
}
