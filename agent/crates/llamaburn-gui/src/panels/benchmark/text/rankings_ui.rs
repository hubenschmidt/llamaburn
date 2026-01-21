//! Text benchmark rankings view

use eframe::egui::{self, Widget};
use llamaburn_services::TextBenchmark;

use crate::panels::benchmark::components::rankings_widget;

/// Text benchmark rankings view
pub struct RankingsView<'a> {
    model: &'a TextBenchmark,
}

impl<'a> RankingsView<'a> {
    pub fn new(model: &'a TextBenchmark) -> Self {
        Self { model }
    }
}

impl Widget for RankingsView<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let response = ui.vertical(|ui| {
            let all_time_ref = self
                .model
                .all_time_best
                .as_ref()
                .map(|(m, t)| (m.as_str(), t));

            rankings_widget(
                ui,
                self.model.model_best_tps.as_ref(),
                all_time_ref,
                &self.model.leaderboard,
                |tps| format!("{:.1} TPS", tps),
            );
        });

        response.response
    }
}
