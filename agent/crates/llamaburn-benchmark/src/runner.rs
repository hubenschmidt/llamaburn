use crate::ollama::OllamaClient;
use futures::StreamExt;
use llamaburn_core::{BenchmarkConfig, BenchmarkMetrics, LlamaBurnError, Result};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BenchmarkEvent {
    Warmup { current: u32, total: u32 },
    Iteration { current: u32, total: u32, prompt: String },
    Token { content: String },
    IterationComplete { metrics: BenchmarkMetrics },
    Done { summary: BenchmarkSummary },
    Cancelled,
    Error { message: String },
}

pub struct BenchmarkRunner {
    client: OllamaClient,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub config: BenchmarkConfig,
    pub metrics: Vec<BenchmarkMetrics>,
    pub summary: BenchmarkSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSummary {
    pub avg_ttft_ms: f64,
    pub avg_tps: f64,
    pub avg_total_ms: f64,
    pub min_tps: f64,
    pub max_tps: f64,
    pub iterations: u32,
}

impl BenchmarkRunner {
    pub fn new(ollama_host: &str) -> Self {
        Self {
            client: OllamaClient::new(ollama_host),
        }
    }

    pub async fn run(&self, config: &BenchmarkConfig, prompts: &[String]) -> Result<BenchmarkResult> {
        tracing::info!("Starting benchmark for model: {}", config.model_id);

        // Warmup runs
        for i in 0..config.warmup_runs {
            tracing::debug!("Warmup run {}/{}", i + 1, config.warmup_runs);
            self.client.warmup(&config.model_id).await?;
        }

        let mut metrics = Vec::with_capacity(config.iterations as usize);

        for i in 0..config.iterations {
            let prompt = &prompts[i as usize % prompts.len()];
            tracing::info!("Iteration {}/{}", i + 1, config.iterations);

            let m = self.run_single(config, prompt).await?;
            metrics.push(m);
        }

        let summary = Self::calculate_summary(&metrics);

        Ok(BenchmarkResult {
            config: config.clone(),
            metrics,
            summary,
        })
    }

    pub async fn run_cancellable(
        &self,
        config: &BenchmarkConfig,
        prompts: &[String],
        cancel_token: CancellationToken,
    ) -> Result<BenchmarkResult> {
        tracing::info!("Starting cancellable benchmark for model: {}", config.model_id);

        // Warmup runs
        for i in 0..config.warmup_runs {
            if cancel_token.is_cancelled() {
                return Err(LlamaBurnError::Cancelled);
            }
            tracing::debug!("Warmup run {}/{}", i + 1, config.warmup_runs);
            self.client.warmup(&config.model_id).await?;
        }

        let mut metrics = Vec::with_capacity(config.iterations as usize);

        for i in 0..config.iterations {
            if cancel_token.is_cancelled() {
                return Err(LlamaBurnError::Cancelled);
            }
            let prompt = &prompts[i as usize % prompts.len()];
            tracing::info!("Iteration {}/{}", i + 1, config.iterations);

            let m = self.run_single(config, prompt).await?;
            metrics.push(m);
        }

        if metrics.is_empty() {
            return Err(LlamaBurnError::Cancelled);
        }

        let summary = Self::calculate_summary(&metrics);

        Ok(BenchmarkResult {
            config: config.clone(),
            metrics,
            summary,
        })
    }

    pub async fn run_streaming(
        &self,
        config: &BenchmarkConfig,
        prompts: &[String],
        cancel_token: CancellationToken,
        tx: mpsc::Sender<BenchmarkEvent>,
    ) {
        // Warmup runs
        for i in 0..config.warmup_runs {
            if cancel_token.is_cancelled() {
                let _ = tx.send(BenchmarkEvent::Cancelled).await;
                return;
            }
            let _ = tx.send(BenchmarkEvent::Warmup {
                current: i + 1,
                total: config.warmup_runs,
            }).await;

            if let Err(e) = self.client.warmup(&config.model_id).await {
                let _ = tx.send(BenchmarkEvent::Error { message: e.to_string() }).await;
                return;
            }
        }

        let mut all_metrics = Vec::with_capacity(config.iterations as usize);

        for i in 0..config.iterations {
            if cancel_token.is_cancelled() {
                let _ = tx.send(BenchmarkEvent::Cancelled).await;
                return;
            }

            let prompt = &prompts[i as usize % prompts.len()];
            let _ = tx.send(BenchmarkEvent::Iteration {
                current: i + 1,
                total: config.iterations,
                prompt: prompt.clone(),
            }).await;

            let start = Instant::now();
            let stream_result = self.client.chat_stream(
                &config.model_id,
                prompt,
                Some(config.temperature),
                config.max_tokens,
            ).await;

            let mut chunk_stream = match stream_result {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(BenchmarkEvent::Error { message: e.to_string() }).await;
                    return;
                }
            };

            let mut eval_count: u64 = 0;
            let mut eval_duration: i64 = 0;

            while let Some(chunk_result) = chunk_stream.next().await {
                if cancel_token.is_cancelled() {
                    let _ = tx.send(BenchmarkEvent::Cancelled).await;
                    return;
                }

                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(BenchmarkEvent::Error { message: e.to_string() }).await;
                        return;
                    }
                };

                if !chunk.content.is_empty() {
                    let _ = tx.send(BenchmarkEvent::Token { content: chunk.content }).await;
                }

                if chunk.done {
                    eval_count = chunk.eval_count.unwrap_or(0);
                    eval_duration = chunk.eval_duration.unwrap_or(0);
                }
            }

            let total_ms = start.elapsed().as_secs_f64() * 1000.0;
            let eval_duration_ns = eval_duration.max(0) as f64;
            let tokens_per_sec = if eval_duration_ns > 0.0 {
                (eval_count as f64) / (eval_duration_ns / 1_000_000_000.0)
            } else {
                0.0
            };

            let metrics = BenchmarkMetrics {
                time_to_first_token_ms: 0.0, // Not available in streaming mode
                inter_token_latency_ms: 0.0,
                tokens_per_sec,
                total_generation_ms: total_ms,
                prompt_eval_ms: 0.0,
                load_duration_ms: 0.0,
                input_sequence_length: 0,
                output_sequence_length: eval_count as u32,
                power_draw_watts: None,
                energy_wh: None,
            };

            let _ = tx.send(BenchmarkEvent::IterationComplete { metrics: metrics.clone() }).await;
            all_metrics.push(metrics);
        }

        let summary = Self::calculate_summary(&all_metrics);
        let _ = tx.send(BenchmarkEvent::Done { summary }).await;
    }

    async fn run_single(&self, config: &BenchmarkConfig, prompt: &str) -> Result<BenchmarkMetrics> {
        let start = Instant::now();

        let response = self
            .client
            .chat(
                &config.model_id,
                prompt,
                Some(config.temperature),
                config.max_tokens,
            )
            .await?;

        let total_ms = start.elapsed().as_secs_f64() * 1000.0;

        let eval_count = response.eval_count.unwrap_or(0) as u32;
        let eval_duration_ns = response.eval_duration.unwrap_or(0).max(0) as f64;
        let load_duration_ns = response.load_duration.unwrap_or(0).max(0) as f64;
        let prompt_eval_ns = response.prompt_eval_duration.unwrap_or(0).max(0) as f64;
        let prompt_eval_count = response.prompt_eval_count.unwrap_or(0) as u32;

        let eval_ms = eval_duration_ns / 1_000_000.0;
        let load_ms = load_duration_ns / 1_000_000.0;
        let prompt_eval_ms = prompt_eval_ns / 1_000_000.0;

        let tokens_per_sec = if eval_duration_ns > 0.0 {
            (eval_count as f64) / (eval_duration_ns / 1_000_000_000.0)
        } else {
            0.0
        };

        // TTFT approximation: load + prompt eval
        let ttft_ms = load_ms + prompt_eval_ms;

        // ITL: (total generation - TTFT) / (tokens - 1)
        let itl_ms = if eval_count > 1 {
            (eval_ms) / (eval_count - 1) as f64
        } else {
            0.0
        };

        Ok(BenchmarkMetrics {
            time_to_first_token_ms: ttft_ms,
            inter_token_latency_ms: itl_ms,
            tokens_per_sec,
            total_generation_ms: total_ms,
            prompt_eval_ms,
            load_duration_ms: load_ms,
            input_sequence_length: prompt_eval_count,
            output_sequence_length: eval_count,
            power_draw_watts: None,
            energy_wh: None,
        })
    }

    fn calculate_summary(metrics: &[BenchmarkMetrics]) -> BenchmarkSummary {
        let n = metrics.len() as f64;

        let avg_ttft_ms = metrics.iter().map(|m| m.time_to_first_token_ms).sum::<f64>() / n;
        let avg_tps = metrics.iter().map(|m| m.tokens_per_sec).sum::<f64>() / n;
        let avg_total_ms = metrics.iter().map(|m| m.total_generation_ms).sum::<f64>() / n;

        let min_tps = metrics
            .iter()
            .map(|m| m.tokens_per_sec)
            .fold(f64::INFINITY, f64::min);
        let max_tps = metrics
            .iter()
            .map(|m| m.tokens_per_sec)
            .fold(f64::NEG_INFINITY, f64::max);

        BenchmarkSummary {
            avg_ttft_ms,
            avg_tps,
            avg_total_ms,
            min_tps,
            max_tps,
            iterations: metrics.len() as u32,
        }
    }
}
