use leptos::prelude::*;

#[component]
pub fn Header() -> impl IntoView {
    view! {
        <header class="header">
            <h1>"LlamaBurn"</h1>
            <span class="subtitle">"LLM Benchmarking Suite"</span>
        </header>
    }
}
