use anyhow::Result;
use clap::{Parser, Subcommand};
use llamaburn_benchmark::BenchmarkRunner;
use llamaburn_core::BenchmarkConfig;
use std::io::{self, BufRead, Write};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "llamaburn")]
#[command(about = "LlamaBurn - LLM Benchmarking Suite", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// List available models
    Models,

    /// Run benchmark tests
    Benchmark {
        /// Model ID to benchmark
        #[arg(short, long)]
        model: String,

        /// Number of iterations
        #[arg(short, long, default_value = "5")]
        iterations: u32,

        /// Number of warmup runs
        #[arg(short, long, default_value = "2")]
        warmup: u32,

        /// Prompt set to use (default, coding, reasoning, factual, creative)
        #[arg(short, long, default_value = "default")]
        prompts: String,

        /// Temperature for generation
        #[arg(short, long, default_value = "0.0")]
        temperature: f32,

        /// Max tokens to generate
        #[arg(long)]
        max_tokens: Option<u32>,

        /// Output format (table, json, csv)
        #[arg(short, long, default_value = "table")]
        output: String,

        /// Ollama host URL
        #[arg(long, default_value = "http://localhost:11434")]
        ollama_host: String,
    },

    /// Show system status
    Status,
}

fn get_ollama_host() -> String {
    std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".to_string())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Models) => cmd_models().await?,
        Some(Commands::Benchmark {
            model,
            iterations,
            warmup,
            prompts,
            temperature,
            max_tokens,
            output,
            ollama_host,
        }) => {
            cmd_benchmark(
                &model,
                iterations,
                warmup,
                &prompts,
                temperature,
                max_tokens,
                &output,
                &ollama_host,
            )
            .await?
        }
        Some(Commands::Status) => cmd_status().await?,
        None => run_interactive().await?,
    }

    Ok(())
}

async fn run_interactive() -> Result<()> {
    display_welcome();
    cmd_status().await?;
    println!();

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("> ");
        stdout.flush()?;

        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            break; // EOF
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        let cmd = parts[0].to_lowercase();

        match cmd.as_str() {
            "help" | "h" | "?" => display_help(),
            "models" | "m" => {
                if let Err(e) = cmd_models().await {
                    println!("Error: {}", e);
                }
            }
            "status" | "s" => {
                if let Err(e) = cmd_status().await {
                    println!("Error: {}", e);
                }
            }
            "benchmark" | "bench" | "b" => {
                if let Err(e) = handle_benchmark_command(&parts[1..]).await {
                    println!("Error: {}", e);
                }
            }
            "detach" => {
                println!();
                println!("  To detach from CLI: Press Ctrl+P, then Ctrl+Q");
                println!("  To reattach: docker attach llamaburn-cli-1");
                println!();
            }
            "exit" | "quit" | "q" => {
                println!();
                println!("  WARNING: This will stop the Docker container.");
                println!("  To keep the container running, use Ctrl+P, Ctrl+Q to detach instead.");
                println!();
                print!("  Are you sure you want to exit? (y/N): ");
                stdout.flush()?;
                let mut confirm = String::new();
                stdin.lock().read_line(&mut confirm)?;
                if confirm.trim().to_lowercase() == "y" {
                    println!("  Goodbye!");
                    break;
                }
            }
            "clear" | "cls" => {
                print!("\x1B[2J\x1B[1;1H");
                stdout.flush()?;
            }
            _ => {
                println!("Unknown command: {}. Type 'help' for available commands.", cmd);
            }
        }
    }

    Ok(())
}

fn display_welcome() {
    println!();
    println!("  ╦  ╦  ╔═╗ ╔╦╗ ╔═╗ ╔╗  ╦ ╦ ╦═╗ ╔╗╔");
    println!("  ║  ║  ╠═╣ ║║║ ╠═╣ ╠╩╗ ║ ║ ╠╦╝ ║║║");
    println!("  ╩═╝╩═╝╩ ╩ ╩ ╩ ╩ ╩ ╚═╝ ╚═╝ ╩╚═ ╝╚╝");
    println!();
    println!("  LLM Benchmarking Suite");
    println!();
    println!("  Use the interactive commands:");
    println!();
    println!("  models, m              # List available Ollama models");
    println!("  benchmark, b <#|name>  # Run benchmark (e.g., `b 1` or `b llama3.1:8b`)");
    println!("  status, s              # Show system status");
    println!("  clear                  # Clear the screen");
    println!("  help                   # Show all command options");
    println!("  detach                 # Show detach instructions");
    println!("  exit, quit, q          # Stop CLI and container");
    println!();
}

fn display_help() {
    println!();
    println!("Available Commands:");
    println!("  models, m              List available Ollama models (with index numbers)");
    println!("  benchmark, b <#|name>  Run benchmark by index or model name");
    println!("    Examples:");
    println!("      b 1                Benchmark model #1 from list");
    println!("      b llama3.1:8b      Benchmark by name");
    println!("    Options:");
    println!("      -i, --iterations   Number of iterations (default: 5)");
    println!("      -w, --warmup       Warmup runs (default: 2)");
    println!("      -p, --prompts      Prompt set: default, coding, reasoning, factual, creative");
    println!("  status, s              Show system status");
    println!("  clear, cls             Clear screen");
    println!("  help, h                Show this help message");
    println!("  detach                 Show how to detach (keep container running)");
    println!("  exit, quit, q          Stop CLI and container");
    println!();
    println!("Tip: Use Ctrl+P, Ctrl+Q to detach without stopping the container");
    println!();
}

async fn resolve_model_id(input: &str) -> Result<String> {
    if let Ok(index) = input.parse::<usize>() {
        let host = get_ollama_host();
        let client = llamaburn_benchmark::ollama::OllamaClient::new(&host);
        let models = client.list_models().await?;
        if index == 0 || index > models.len() {
            anyhow::bail!("Invalid model index: {}. Use 1-{}", index, models.len());
        }
        return Ok(models[index - 1].id.clone());
    }
    Ok(input.to_string())
}

async fn handle_benchmark_command(args: &[&str]) -> Result<()> {
    if args.is_empty() {
        println!("Usage: benchmark <model> [options]");
        println!("  Example: benchmark 1 -i 3  (by index)");
        println!("  Example: benchmark llama3.1:8b -i 3  (by name)");
        return Ok(());
    }

    let model = resolve_model_id(args[0]).await?;
    let mut iterations = 5u32;
    let mut warmup = 2u32;
    let mut prompts = "default";
    let mut temperature = 0.0f32;
    let mut max_tokens: Option<u32> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i] {
            "-i" | "--iterations" => {
                if i + 1 < args.len() {
                    iterations = args[i + 1].parse().unwrap_or(5);
                    i += 1;
                }
            }
            "-w" | "--warmup" => {
                if i + 1 < args.len() {
                    warmup = args[i + 1].parse().unwrap_or(2);
                    i += 1;
                }
            }
            "-p" | "--prompts" => {
                if i + 1 < args.len() {
                    prompts = args[i + 1];
                    i += 1;
                }
            }
            "-t" | "--temperature" => {
                if i + 1 < args.len() {
                    temperature = args[i + 1].parse().unwrap_or(0.0);
                    i += 1;
                }
            }
            "-m" | "--max-tokens" => {
                if i + 1 < args.len() {
                    max_tokens = args[i + 1].parse().ok();
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    let ollama_host = get_ollama_host();
    cmd_benchmark(&model, iterations, warmup, prompts, temperature, max_tokens, "table", &ollama_host).await
}

async fn cmd_models() -> Result<()> {
    let host = get_ollama_host();
    let client = llamaburn_benchmark::ollama::OllamaClient::new(&host);
    let models = client.list_models().await?;

    println!();
    println!("Available Models:");
    println!("{:-<65}", "");
    println!("  {:<4} {:<40} {}", "#", "ID", "Quantization");
    println!("{:-<65}", "");
    for (i, m) in models.iter().enumerate() {
        let quant = m.quantization.as_deref().unwrap_or("-");
        println!("  {:<4} {:<40} {}", i + 1, m.id, quant);
    }
    println!();
    println!("  Use: benchmark <#> or <ID> (e.g., `b 1` or `b gpt-oss:latest`)");
    println!();

    Ok(())
}

async fn cmd_benchmark(
    model: &str,
    iterations: u32,
    warmup: u32,
    prompts: &str,
    temperature: f32,
    max_tokens: Option<u32>,
    output_format: &str,
    ollama_host: &str,
) -> Result<()> {
    let runner = BenchmarkRunner::new(ollama_host);

    let config = BenchmarkConfig {
        benchmark_type: Default::default(),
        model_id: model.to_string(),
        iterations,
        warmup_runs: warmup,
        prompt_set: prompts.to_string(),
        temperature,
        max_tokens,
        top_p: None,
        top_k: None,
    };

    let test_prompts = get_prompts(prompts);

    println!();
    println!("Running benchmark...");
    println!("  Model: {}", model);
    println!("  Iterations: {}", iterations);
    println!("  Warmup: {}", warmup);
    println!();

    let result = runner.run(&config, &test_prompts).await?;

    match output_format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&result.summary)?);
        }
        "csv" => {
            println!("metric,value");
            println!("avg_ttft_ms,{:.2}", result.summary.avg_ttft_ms);
            println!("avg_tps,{:.2}", result.summary.avg_tps);
            println!("avg_total_ms,{:.2}", result.summary.avg_total_ms);
            println!("min_tps,{:.2}", result.summary.min_tps);
            println!("max_tps,{:.2}", result.summary.max_tps);
        }
        _ => {
            println!("Results:");
            println!("{:-<40}", "");
            println!("  Avg TTFT:      {:.2} ms", result.summary.avg_ttft_ms);
            println!("  Avg TPS:       {:.2} tokens/sec", result.summary.avg_tps);
            println!("  Avg Total:     {:.2} ms", result.summary.avg_total_ms);
            println!("  Min TPS:       {:.2}", result.summary.min_tps);
            println!("  Max TPS:       {:.2}", result.summary.max_tps);
            println!("  Iterations:    {}", result.summary.iterations);
        }
    }
    println!();

    Ok(())
}

async fn cmd_status() -> Result<()> {
    let host = get_ollama_host();
    println!("System Status:");
    println!("{:-<40}", "");
    println!("  Ollama Host: {}", host);

    let client = llamaburn_benchmark::ollama::OllamaClient::new(&host);
    match client.list_models().await {
        Ok(models) => {
            println!("  Ollama: connected ({} models available)", models.len());
        }
        Err(e) => {
            println!("  Ollama: disconnected ({})", e);
        }
    }

    Ok(())
}

fn get_prompts(set: &str) -> Vec<String> {
    match set {
        "coding" => vec![
            "Write a function to reverse a string in Python.".to_string(),
            "Implement a binary search algorithm.".to_string(),
            "Create a simple HTTP server in Rust.".to_string(),
        ],
        "reasoning" => vec![
            "If all roses are flowers and some flowers fade quickly, can we conclude that some roses fade quickly?".to_string(),
            "What is 17 * 23?".to_string(),
            "A train leaves at 9am traveling at 60mph. Another leaves at 10am at 80mph. When do they meet?".to_string(),
        ],
        "factual" => vec![
            "What is the capital of France?".to_string(),
            "Who wrote Romeo and Juliet?".to_string(),
            "What is the speed of light in vacuum?".to_string(),
        ],
        "creative" => vec![
            "Write a haiku about programming.".to_string(),
            "Describe a futuristic city in 3 sentences.".to_string(),
            "Create a short story opening about a robot.".to_string(),
        ],
        _ => vec![
            "Hello, how are you?".to_string(),
            "Explain what machine learning is in simple terms.".to_string(),
            "What are three benefits of exercise?".to_string(),
            "Write a short poem about the moon.".to_string(),
            "Describe the process of photosynthesis.".to_string(),
        ],
    }
}
