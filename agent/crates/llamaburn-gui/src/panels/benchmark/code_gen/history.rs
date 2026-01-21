//! History and rankings methods for code benchmark panel

use llamaburn_services::{CodeBenchmark, Language};
use llamaburn_services::HistoryService;

use super::CodeGenBenchmarkPanel;

impl CodeGenBenchmarkPanel {
    /// Refresh rankings if language changed
    pub fn refresh_rankings(&self, model: &mut CodeBenchmark, history_service: &HistoryService) {
        let Some(lang) = self.selected_languages.first().copied() else {
            return;
        };

        if model.last_language_for_rankings == Some(lang) {
            return;
        }

        model.last_language_for_rankings = Some(lang);
        model.leaderboard = history_service
            .get_code_leaderboard(lang, 5)
            .unwrap_or_default();
    }

    /// Force refresh rankings regardless of language change
    pub fn force_refresh_rankings(&self, model: &mut CodeBenchmark, history_service: &HistoryService) {
        model.last_language_for_rankings = None;
        let lang = self
            .selected_languages
            .first()
            .copied()
            .unwrap_or(Language::Python);
        model.leaderboard = history_service
            .get_code_leaderboard(lang, 5)
            .unwrap_or_default();
    }
}
