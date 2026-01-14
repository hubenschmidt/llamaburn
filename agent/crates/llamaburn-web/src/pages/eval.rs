use leptos::prelude::*;

#[component]
pub fn EvalPage() -> impl IntoView {
    view! {
        <div class="page eval-page">
            <h2>"Accuracy Evaluator"</h2>

            <div class="config-panel">
                <div class="form-group">
                    <label>"Model"</label>
                    <input type="text" value="llama3.1:8b" />
                </div>

                <div class="form-group">
                    <label>"Eval Set"</label>
                    <select>
                        <option value="general_knowledge">"General Knowledge"</option>
                        <option value="coding">"Coding"</option>
                        <option value="reasoning">"Reasoning"</option>
                    </select>
                </div>

                <div class="form-group">
                    <label>"Judge"</label>
                    <select>
                        <option value="claude">"Claude"</option>
                        <option value="openai">"OpenAI GPT"</option>
                    </select>
                </div>

                <div class="form-group">
                    <label>
                        <input type="checkbox" />
                        " Allow Web Search"
                    </label>
                </div>

                <button class="run-btn">"Run Evaluation"</button>
            </div>

            <div class="results-panel">
                <h3>"Scores"</h3>
                <p class="placeholder">"Run an evaluation to see scores"</p>
            </div>
        </div>
    }
}
