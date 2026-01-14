use crate::api::{self, BenchmarkSummary};
use leptos::prelude::*;
use llamaburn_core::{model::ModelConfig, BenchmarkType};
use wasm_bindgen::{closure::Closure, JsCast};
use web_sys::{EventSource, MessageEvent};

#[component]
pub fn BenchmarkPage() -> impl IntoView {
    let (models, set_models) = signal(Vec::<ModelConfig>::new());
    let (selected_model, set_selected_model) = signal(String::new());
    let (benchmark_type, set_benchmark_type) = signal(BenchmarkType::Text);
    let (iterations, set_iterations) = signal(5u32);
    let (warmup, set_warmup) = signal(2u32);
    let (temperature, set_temperature) = signal(0.0f32);
    let (running, set_running) = signal(false);
    let (loading, set_loading) = signal(false);
    let (unloading, set_unloading) = signal(false);
    let (cancelling, set_cancelling) = signal(false);
    let (result, set_result) = signal(None::<BenchmarkSummary>);
    let (error, set_error) = signal(None::<String>);
    let (live_output, set_live_output) = signal(String::new());
    let (progress, set_progress) = signal(String::new());

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
        let model = selected_model.get();
        if model.is_empty() {
            return;
        }

        set_running.set(true);
        set_error.set(None);
        set_result.set(None);
        set_live_output.set(String::new());
        set_progress.set(String::new());

        let iters = iterations.get();
        let warm = warmup.get();
        let temp = temperature.get();

        let url = format!(
            "/api/benchmark?model={}&iterations={}&warmup={}&temp={}",
            model, iters, warm, temp
        );

        let Some(event_source) = EventSource::new(&url).ok() else {
            set_running.set(false);
            set_error.set(Some("Failed to connect".to_string()));
            return;
        };

        let es_clone = event_source.clone();
        let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
            let Some(data) = e.data().as_string() else { return };
            let Ok(event) = serde_json::from_str::<serde_json::Value>(&data) else { return };

            let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");

            match event_type {
                "warmup" => {
                    let current = event.get("current").and_then(|c| c.as_u64()).unwrap_or(0);
                    let total = event.get("total").and_then(|t| t.as_u64()).unwrap_or(0);
                    set_progress.set(format!("Warmup {}/{}", current, total));
                }
                "iteration" => {
                    let current = event.get("current").and_then(|c| c.as_u64()).unwrap_or(0);
                    let total = event.get("total").and_then(|t| t.as_u64()).unwrap_or(0);
                    set_progress.set(format!("Iteration {}/{}", current, total));
                    set_live_output.update(|s| s.push_str("\n\n--- New Iteration ---\n"));
                }
                "token" => {
                    let content = event.get("content").and_then(|c| c.as_str()).unwrap_or("");
                    set_live_output.update(|s| s.push_str(content));
                }
                "done" => {
                    if let Some(summary) = event.get("summary") {
                        if let Ok(s) = serde_json::from_value::<BenchmarkSummary>(summary.clone()) {
                            set_result.set(Some(s));
                        }
                    }
                    set_running.set(false);
                    set_progress.set("Complete".to_string());
                    es_clone.close();
                }
                "error" => {
                    let msg = event.get("message").and_then(|m| m.as_str()).unwrap_or("Unknown error");
                    set_error.set(Some(msg.to_string()));
                    set_running.set(false);
                    es_clone.close();
                }
                _ => {}
            }
        }) as Box<dyn FnMut(MessageEvent)>);

        event_source.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();

        let es_err = event_source.clone();
        let onerror = Closure::wrap(Box::new(move |_| {
            set_running.set(false);
            es_err.close();
        }) as Box<dyn FnMut(web_sys::Event)>);

        event_source.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        onerror.forget();
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

            <div class="type-selector">
                {BenchmarkType::all().iter().map(|t| {
                    let t = *t;
                    let is_selected = move || benchmark_type.get() == t;
                    let is_implemented = t.is_implemented();
                    view! {
                        <button
                            class=move || if is_selected() { "type-tab selected" } else { "type-tab" }
                            disabled=move || !is_implemented || running.get()
                            on:click=move |_| set_benchmark_type.set(t)
                        >
                            {t.label()}
                            {(!is_implemented).then(|| view! { <span class="coming-soon">"Soon"</span> })}
                        </button>
                    }
                }).collect::<Vec<_>>()}
            </div>

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

            <div class="live-output-panel">
                <div class="live-output-header">
                    <h3>"Live Output"</h3>
                    <span class="progress-indicator">{progress}</span>
                </div>
                <pre class="live-output">{live_output}</pre>
            </div>

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
                                    <td>"Avg TPS"</td>
                                    <td>{format!("{:.2} tokens/sec", r.avg_tps)}</td>
                                </tr>
                                <tr>
                                    <td>"Avg Total"</td>
                                    <td>{format!("{:.2} ms", r.avg_total_ms)}</td>
                                </tr>
                                <tr>
                                    <td>"Min TPS"</td>
                                    <td>{format!("{:.2}", r.min_tps)}</td>
                                </tr>
                                <tr>
                                    <td>"Max TPS"</td>
                                    <td>{format!("{:.2}", r.max_tps)}</td>
                                </tr>
                                <tr>
                                    <td>"Iterations"</td>
                                    <td>{r.iterations.to_string()}</td>
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
