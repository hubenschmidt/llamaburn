use leptos::prelude::*;

#[component]
pub fn BenchmarkPage() -> impl IntoView {
    let (model, set_model) = signal(String::from("llama3.1:8b"));
    let (iterations, set_iterations) = signal(5u32);
    let (running, set_running) = signal(false);

    view! {
        <div class="page benchmark-page">
            <h2>"Benchmark Runner"</h2>

            <div class="config-panel">
                <div class="form-group">
                    <label>"Model"</label>
                    <input
                        type="text"
                        prop:value=model
                        on:input=move |ev| set_model.set(event_target_value(&ev))
                    />
                </div>

                <div class="form-group">
                    <label>"Iterations"</label>
                    <input
                        type="number"
                        prop:value=move || iterations.get().to_string()
                        on:input=move |ev| {
                            if let Ok(n) = event_target_value(&ev).parse() {
                                set_iterations.set(n);
                            }
                        }
                    />
                </div>

                <button
                    class="run-btn"
                    disabled=running
                    on:click=move |_| {
                        set_running.set(true);
                        // TODO: Run benchmark
                    }
                >
                    {move || if running.get() { "Running..." } else { "Run Benchmark" }}
                </button>
            </div>

            <div class="results-panel">
                <h3>"Results"</h3>
                <p class="placeholder">"Run a benchmark to see results"</p>
            </div>
        </div>
    }
}
