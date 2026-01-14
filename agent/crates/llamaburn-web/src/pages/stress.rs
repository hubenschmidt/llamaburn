use leptos::prelude::*;

#[component]
pub fn StressPage() -> impl IntoView {
    view! {
        <div class="page stress-page">
            <h2>"Stress Tester"</h2>

            <div class="config-panel">
                <div class="form-group">
                    <label>"Mode"</label>
                    <select>
                        <option value="ramp">"Ramp"</option>
                        <option value="sweep">"Sweep"</option>
                        <option value="sustained">"Sustained"</option>
                        <option value="spike">"Spike"</option>
                    </select>
                </div>

                <div class="form-group">
                    <label>"Max Concurrency"</label>
                    <input type="number" value="10" />
                </div>

                <button class="run-btn">"Start Stress Test"</button>
            </div>

            <div class="metrics-panel">
                <h3>"Live Metrics"</h3>
                <p class="placeholder">"Start a test to see live metrics"</p>
            </div>
        </div>
    }
}
