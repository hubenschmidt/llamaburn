use crate::ollama::OllamaClient;
use llamaburn_core::{BenchmarkConfig, BenchmarkMetrics, Result};
use serde::{Deserialize, Serialize};
use std::time::Instant;

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
        let eval_duration_ns = response.eval_duration.unwrap_or(0) as f64;
        let load_duration_ns = response.load_duration.unwrap_or(0) as f64;
        let prompt_eval_ns = response.prompt_eval_duration.unwrap_or(0) as f64;
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
