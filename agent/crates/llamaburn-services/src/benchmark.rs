use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::runtime::Runtime;
use tokio::sync::mpsc as tokio_mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

use crate::runners::{BenchmarkEvent, BenchmarkRunner, BenchmarkSummary};
use llamaburn_core::{
    AudioBenchmark, AudioMode, TextBenchmarkConfig, BenchmarkType, ModelList, TextBenchmark,
    TextBenchmarkResult, WhisperModel,
};

use crate::{BenchmarkHistoryEntry, HistoryService};

/// Default prompts for text benchmarks
const DEFAULT_PROMPTS: &[&str] = &[
    "Explain the concept of recursion in programming.",
    "What are the benefits of functional programming?",
    "Describe how a hash table works.",
    "What is the difference between a stack and a queue?",
    "Explain the CAP theorem in distributed systems.",
];

/// Stateless benchmark service - operates on models via &mut references
pub struct BenchmarkService {
    ollama_host: String,
}

impl BenchmarkService {
    pub fn new(ollama_host: impl Into<String>) -> Self {
        Self {
            ollama_host: ollama_host.into(),
        }
    }

    pub fn default_host() -> Self {
        Self::new("http://localhost:11434")
    }

    /// Start a streaming benchmark run
    #[instrument(skip(self, config), fields(model = %config.model_id, iterations = config.iterations))]
    pub fn run_streaming(
        &self,
        config: TextBenchmarkConfig,
    ) -> (Receiver<BenchmarkEvent>, Arc<CancellationToken>) {
        info!("Starting streaming benchmark");

        let (std_tx, std_rx) = channel();
        let cancel_token = Arc::new(CancellationToken::new());
        let cancel_clone = cancel_token.clone();
        let host = self.ollama_host.clone();

        thread::spawn(move || {
            let rt = match Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    error!("Failed to create tokio runtime: {}", e);
                    let _ = std_tx.send(BenchmarkEvent::Error {
                        message: format!("Runtime error: {}", e),
                    });
                    return;
                }
            };

            rt.block_on(async {
                let runner = BenchmarkRunner::new(&host);
                let (tokio_tx, mut tokio_rx) = tokio_mpsc::channel(100);

                let prompts: Vec<String> = DEFAULT_PROMPTS.iter().map(|s| s.to_string()).collect();

                let runner_cancel = (*cancel_clone).clone();
                tokio::spawn(async move {
                    runner
                        .run_streaming(&config, &prompts, runner_cancel, tokio_tx)
                        .await;
                });

                while let Some(event) = tokio_rx.recv().await {
                    debug!("Benchmark event: {:?}", std::mem::discriminant(&event));
                    if std_tx.send(event).is_err() {
                        debug!("Benchmark receiver dropped");
                        break;
                    }
                }

                info!("Benchmark streaming complete");
            });
        });

        (std_rx, cancel_token)
    }

    /// Cancel a running benchmark
    pub fn cancel(token: &CancellationToken) {
        info!("Cancelling benchmark");
        token.cancel();
    }

    // =========================================================================
    // Text Benchmark Controller Methods
    // =========================================================================

    /// Start a text benchmark - takes &mut model, returns receiver
    pub fn start_text_benchmark(
        &self,
        text: &mut TextBenchmark,
        models: &ModelList,
    ) -> Option<Receiver<BenchmarkEvent>> {
        text.start(&models.selected);
        text.append_output(&format!(
            "Starting text benchmark: {} iterations, {} warmup, temp={:.1}\n",
            text.config.iterations, text.config.warmup_runs, text.config.temperature
        ));

        let (rx, _cancel_token) = self.run_streaming(text.config.clone());
        Some(rx)
    }

    /// Cancel text benchmark
    pub fn cancel_text_benchmark(text: &mut TextBenchmark) {
        text.stop();
    }

    /// Poll for text benchmark events
    pub fn poll_text_benchmark(
        &self,
        text: &mut TextBenchmark,
        rx: &mut Option<Receiver<BenchmarkEvent>>,
        history: &HistoryService,
    ) {
        let Some(receiver) = rx.take() else { return };

        loop {
            match receiver.try_recv() {
                Ok(event) => Self::handle_text_event(text, event, history, rx),
                Err(TryRecvError::Empty) => {
                    *rx = Some(receiver);
                    break;
                }
                Err(TryRecvError::Disconnected) => {
                    text.stop();
                    break;
                }
            }
        }
    }

    fn handle_text_event(
        text: &mut TextBenchmark,
        event: BenchmarkEvent,
        history: &HistoryService,
        rx: &mut Option<Receiver<BenchmarkEvent>>,
    ) {
        match event {
            BenchmarkEvent::Warmup { current, total } => {
                text.set_progress(format!("Warmup {}/{}", current, total));
            }
            BenchmarkEvent::Iteration { current, total, prompt: _ } => {
                text.set_progress(format!("Iteration {}/{}", current, total));
            }
            BenchmarkEvent::Token { content } => {
                text.append_output(&content);
            }
            BenchmarkEvent::IterationComplete { metrics } => {
                text.append_output(&format!(
                    "\n[Iteration {}] {:.2} t/s, TTFT: {:.0}ms, Total: {:.0}ms\n",
                    text.collected_metrics.len() + 1,
                    metrics.tokens_per_sec,
                    metrics.time_to_first_token_ms,
                    metrics.total_generation_ms
                ));
                text.add_metrics(metrics);
            }
            BenchmarkEvent::Done { summary } => {
                let result = TextBenchmarkResult {
                    avg_tps: summary.avg_tps,
                    avg_ttft_ms: summary.avg_ttft_ms,
                    avg_total_ms: summary.avg_total_ms,
                    min_tps: summary.min_tps,
                    max_tps: summary.max_tps,
                    iterations: text.config.iterations,
                };

                text.append_output(&format!(
                    "\n✅ Complete: {:.2} t/s avg ({:.2}-{:.2})\n",
                    result.avg_tps, result.min_tps, result.max_tps
                ));

                Self::save_text_history(text, &summary, history);
                text.set_result(result);
                *rx = None;
                text.set_progress(String::new());
                text.last_model_for_rankings.clear();
            }
            BenchmarkEvent::Cancelled => {
                text.append_output("\n⚠️ Benchmark cancelled\n");
                text.stop();
                *rx = None;
                text.set_progress(String::new());
            }
            BenchmarkEvent::Error { message } => {
                text.append_output(&format!("\n❌ Error: {}\n", message));
                text.set_error(Some(message));
                text.stop();
                *rx = None;
                text.set_progress(String::new());
            }
        }
    }

    fn save_text_history(text: &TextBenchmark, summary: &BenchmarkSummary, history: &HistoryService) {
        let config = &text.config;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let entry = BenchmarkHistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp,
            benchmark_type: BenchmarkType::Text,
            model_id: config.model_id.clone(),
            config: config.clone(),
            summary: summary.clone(),
            metrics: text.collected_metrics.clone(),
        };

        if let Err(e) = history.insert(&entry) {
            warn!("Failed to save benchmark history: {}", e);
        } else {
            info!("Saved benchmark result to history: {}", entry.id);
        }
    }

    /// Check if rankings need refresh, and refresh if needed
    pub fn maybe_refresh_text_rankings(
        &self,
        text: &mut TextBenchmark,
        models: &ModelList,
        history: &HistoryService,
    ) {
        let selected = &models.selected;
        if selected.is_empty() || *selected == text.last_model_for_rankings {
            return;
        }
        text.last_model_for_rankings = selected.clone();
        Self::refresh_text_rankings(text, selected, history);
    }

    /// Force refresh rankings
    pub fn refresh_text_rankings(
        text: &mut TextBenchmark,
        selected_model: &str,
        history: &HistoryService,
    ) {
        let model_best = history
            .get_best_for_model(selected_model, BenchmarkType::Text)
            .ok()
            .flatten();

        let leaderboard = history
            .get_leaderboard(BenchmarkType::Text, 10)
            .unwrap_or_default();

        let all_time = leaderboard.first().cloned();
        text.set_rankings(model_best, all_time, leaderboard);
    }

    // =========================================================================
    // Audio Benchmark Controller Methods
    // =========================================================================

    /// Check if audio rankings need refresh, and refresh if needed
    pub fn maybe_refresh_audio_rankings(&self, audio: &mut AudioBenchmark, history: &HistoryService) {
        let Some(whisper_model) = audio.whisper_model else {
            return;
        };

        if audio.last_model_for_rankings == Some(whisper_model) {
            return;
        }

        audio.last_model_for_rankings = Some(whisper_model);
        Self::refresh_audio_rankings(audio, whisper_model, history);
    }

    /// Force refresh audio rankings
    pub fn refresh_audio_rankings(
        audio: &mut AudioBenchmark,
        whisper_model: WhisperModel,
        history: &HistoryService,
    ) {
        let model_id = format!("whisper-{}", whisper_model.label().to_lowercase());

        let model_best = history
            .get_best_audio_for_model(&model_id, AudioMode::Stt)
            .ok()
            .flatten();

        let all_time_best = history
            .get_all_time_best_audio(AudioMode::Stt)
            .ok()
            .flatten();

        let leaderboard = history
            .get_audio_leaderboard(AudioMode::Stt, 5)
            .unwrap_or_default();

        audio.set_rankings(model_best, all_time_best, leaderboard);
    }

    /// Force refresh audio rankings (clears cache first)
    pub fn force_refresh_audio_rankings(&self, audio: &mut AudioBenchmark, history: &HistoryService) {
        audio.last_model_for_rankings = None;
        self.maybe_refresh_audio_rankings(audio, history);
    }
}

impl Default for BenchmarkService {
    fn default() -> Self {
        Self::default_host()
    }
}
