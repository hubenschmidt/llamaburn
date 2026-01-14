use leptos::prelude::*;

#[component]
pub fn HomePage() -> impl IntoView {
    view! {
        <div class="page home-page">
            <h2>"Welcome to LlamaBurn"</h2>
            <p>"A comprehensive benchmarking suite for local LLM models."</p>

            <div class="features">
                <div class="feature">
                    <h3>"Benchmark"</h3>
                    <p>"Measure TTFT, TPS, and latency metrics"</p>
                </div>
                <div class="feature">
                    <h3>"Stress Test"</h3>
                    <p>"Find capacity limits and failure thresholds"</p>
                </div>
                <div class="feature">
                    <h3>"Eval"</h3>
                    <p>"Evaluate accuracy with LLM-as-Judge"</p>
                </div>
            </div>
        </div>
    }
}
