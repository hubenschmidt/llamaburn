use crate::api::{self, BenchmarkRequest, BenchmarkResult};
use leptos::prelude::*;
use llamaburn_core::model::ModelConfig;

#[component]
pub fn BenchmarkPage() -> impl IntoView {
    let (models, set_models) = signal(Vec::<ModelConfig>::new());
    let (selected_model, set_selected_model) = signal(String::new());
    let (iterations, set_iterations) = signal(5u32);
    let (warmup, set_warmup) = signal(2u32);
    let (temperature, set_temperature) = signal(0.0f32);
    let (running, set_running) = signal(false);
    let (loading, set_loading) = signal(false);
    let (unloading, set_unloading) = signal(false);
    let (cancelling, set_cancelling) = signal(false);
    let (result, set_result) = signal(None::<BenchmarkResult>);
    let (error, set_error) = signal(None::<String>);

    // Fetch models on mount
    Effect::new(move || {
        wasm_bindgen_futures::spawn_local(async move {
            match api::fetch_models().await {
                Ok(m) => set_models.set(m),
                Err(e) => set_error.set(Some(e)),
            }
        });
    });

    let run_benchmark = move |_| {
        set_running.set(true);
        set_error.set(None);
        set_result.set(None);

        let model_id = selected_model.get();
        let iters = iterations.get();
        let warm = warmup.get();
        let temp = temperature.get();

        wasm_bindgen_futures::spawn_local(async move {
            let req = BenchmarkRequest {
                model_id,
                iterations: Some(iters),
                warmup_runs: Some(warm),
                temperature: Some(temp),
            };

            match api::run_benchmark(req).await {
                Ok(r) => set_result.set(Some(r)),
                Err(e) if e.contains("cancelled") => {} // Cancelled, no error to show
                Err(e) => set_error.set(Some(e)),
            }
            set_running.set(false);
            set_cancelling.set(false);
        });
    };

    let unload_model = move |_| {
        set_unloading.set(true);
        set_error.set(None);
        let model_id = selected_model.get();

        wasm_bindgen_futures::spawn_local(async move {
            match api::unload_model(model_id).await {
                Ok(_) => set_selected_model.set(String::new()),
                Err(e) => set_error.set(Some(e)),
            }
            set_unloading.set(false);
        });
    };

    let cancel_benchmark = move |_| {
        set_cancelling.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            let _ = api::cancel_benchmark().await;
        });
    };

    let on_model_change = move |ev: web_sys::Event| {
        let new_model = event_target_value(&ev);
        let prev_model = selected_model.get();

        set_selected_model.set(new_model.clone());

        if new_model.is_empty() {
            return;
        }

        set_loading.set(true);
        set_error.set(None);

        wasm_bindgen_futures::spawn_local(async move {
            if !prev_model.is_empty() {
                let _ = api::unload_model(prev_model).await;
            }
            match api::load_model(new_model).await {
                Ok(_) => {}
                Err(e) => set_error.set(Some(e)),
            }
            set_loading.set(false);
        });
    };

    view! {
        <div class="page benchmark-page">
            <h2>"Benchmark Runner"</h2>

            <div class="config-panel">
                <div class="form-group">
                    <label>"Model"</label>
                    <select
                        prop:value=selected_model
                        disabled=move || loading.get() || running.get() || unloading.get()
                        on:change=on_model_change
                    >
                        <option value="">"Select a model..."</option>
                        {move || models.get().into_iter().map(|m| {
                            let id = m.id.clone();
                            let display = format!("{} ({})", m.id, m.quantization.unwrap_or_default());
                            view! {
                                <option value=id.clone()>{display}</option>
                            }
                        }).collect::<Vec<_>>()}
                    </select>
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

                <div class="form-group">
                    <label>"Warmup Runs"</label>
                    <input
                        type="number"
                        prop:value=move || warmup.get().to_string()
                        on:input=move |ev| {
                            if let Ok(n) = event_target_value(&ev).parse() {
                                set_warmup.set(n);
                            }
                        }
                    />
                </div>

                <div class="form-group">
                    <label>"Temperature"</label>
                    <input
                        type="number"
                        step="0.1"
                        min="0"
                        max="2"
                        prop:value=move || temperature.get().to_string()
                        on:input=move |ev| {
                            if let Ok(n) = event_target_value(&ev).parse() {
                                set_temperature.set(n);
                            }
                        }
                    />
                </div>

                <div class="button-group">
                    <button
                        class="run-btn"
                        disabled=move || running.get() || loading.get() || unloading.get() || selected_model.get().is_empty()
                        on:click=run_benchmark
                    >
                        {move || if running.get() {
                            view! { <span class="loading"><span class="spinner"></span>" Running..."</span> }.into_any()
                        } else if loading.get() {
                            view! { <span class="loading"><span class="spinner"></span>" Loading..."</span> }.into_any()
                        } else {
                            view! { <span>"Run Benchmark"</span> }.into_any()
                        }}
                    </button>
                    {move || running.get().then(|| view! {
                        <button
                            class="cancel-btn"
                            disabled=move || cancelling.get()
                            on:click=cancel_benchmark
                        >
                            {move || if cancelling.get() { "Cancelling..." } else { "Cancel" }}
                        </button>
                    })}
                    <button
                        class="unload-btn"
                        disabled=move || running.get() || loading.get() || unloading.get() || selected_model.get().is_empty()
                        on:click=unload_model
                    >
                        {move || if unloading.get() {
                            view! { <span class="loading"><span class="spinner"></span>" Unloading..."</span> }.into_any()
                        } else {
                            view! { <span>"Unload Model"</span> }.into_any()
                        }}
                    </button>
                </div>
            </div>

            {move || error.get().map(|e| view! {
                <div class="error-panel">
                    <p style="color: var(--error);">"Error: " {e}</p>
                </div>
            })}

            <div class="results-panel">
                <h3>"Results"</h3>
                {move || match result.get() {
                    Some(r) => view! {
                        <table class="results-table">
                            <thead>
                                <tr>
                                    <th>"Metric"</th>
                                    <th>"Value"</th>
                                </tr>
                            </thead>
                            <tbody>
                                <tr>
                                    <td>"Avg TTFT"</td>
                                    <td>{format!("{:.2} ms", r.summary.avg_ttft_ms)}</td>
                                </tr>
                                <tr>
                                    <td>"Avg TPS"</td>
                                    <td>{format!("{:.2} tokens/sec", r.summary.avg_tps)}</td>
                                </tr>
                                <tr>
                                    <td>"Avg Total"</td>
                                    <td>{format!("{:.2} ms", r.summary.avg_total_ms)}</td>
                                </tr>
                                <tr>
                                    <td>"Min TPS"</td>
                                    <td>{format!("{:.2}", r.summary.min_tps)}</td>
                                </tr>
                                <tr>
                                    <td>"Max TPS"</td>
                                    <td>{format!("{:.2}", r.summary.max_tps)}</td>
                                </tr>
                                <tr>
                                    <td>"Iterations"</td>
                                    <td>{r.summary.iterations.to_string()}</td>
                                </tr>
                            </tbody>
                        </table>
                    }.into_any(),
                    None => view! {
                        <p class="placeholder">"Run a benchmark to see results"</p>
                    }.into_any()
                }}
            </div>
        </div>
    }
}
