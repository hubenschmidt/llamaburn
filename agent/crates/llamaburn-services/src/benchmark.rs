use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread;

use tokio::runtime::Runtime;
use tokio::sync::mpsc as tokio_mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument};

use crate::runners::{BenchmarkEvent, BenchmarkRunner};
use llamaburn_core::TextBenchmarkConfig;

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
}

impl Default for BenchmarkService {
    fn default() -> Self {
        Self::default_host()
    }
}
