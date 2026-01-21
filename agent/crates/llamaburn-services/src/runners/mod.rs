mod code_executor;
mod code_runner;
mod ollama_client;
mod text_runner;

pub use code_executor::{CodeExecutor, CodeExecutorError, TestResult};
pub use code_runner::{run_tests_only, CodeBenchmarkEvent, CodeBenchmarkResult, CodeBenchmarkRunner};
pub use ollama_client::{code_output_schema, StructuredCodeResponse};
pub use text_runner::{BenchmarkEvent, BenchmarkResult, BenchmarkRunner, BenchmarkSummary};
