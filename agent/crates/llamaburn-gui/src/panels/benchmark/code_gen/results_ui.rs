//! Results UI for code benchmark panel

use eframe::egui;

use llamaburn_services::CodeBenchmark;

use super::CodeGenBenchmarkPanel;
use crate::panels::benchmark::components::rankings_widget;

impl CodeGenBenchmarkPanel {
    pub fn render_code_results(&self, ui: &mut egui::Ui, model: &CodeBenchmark) {
        ui.label(egui::RichText::new("Results").strong());

        let Some(summary) = &model.summary else {
            ui.label("No results yet");
            return;
        };

        ui.label(format!("Pass Rate: {:.1}%", summary.pass_rate * 100.0));
        ui.label(format!(
            "Solved: {}/{}",
            summary.problems_solved, summary.problems_total
        ));
        ui.label(format!("Avg TPS: {:.1}", summary.avg_tps));
        ui.label(format!(
            "Avg Exec Time: {:.1}ms",
            summary.avg_execution_time_ms
        ));

        if model.metrics.is_empty() {
            return;
        }

        ui.add_space(10.0);
        ui.label(egui::RichText::new("Per-Problem Results").small());

        for metrics in &model.metrics {
            let status = if metrics.tests_passed == metrics.tests_total {
                "PASS"
            } else {
                "FAIL"
            };
            ui.label(format!(
                "{} {} ({}/{})",
                status, metrics.problem_id, metrics.tests_passed, metrics.tests_total
            ));
        }
    }

    pub fn render_code_rankings(&self, ui: &mut egui::Ui, model: &CodeBenchmark) {
        if model.leaderboard.is_empty() {
            ui.label("No rankings yet");
            return;
        }

        rankings_widget(
            ui,
            model.model_best_pass_rate.as_ref(),
            model.all_time_best.as_ref().map(|(s, r)| (s.as_str(), r)),
            &model.leaderboard,
            |pass_rate| format!("{:.1}%", pass_rate * 100.0),
        );
    }
}
