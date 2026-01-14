use leptos::prelude::*;
use leptos_router::components::A;

#[component]
pub fn NavTabs() -> impl IntoView {
    view! {
        <nav class="nav-tabs">
            <A href="/" class="nav-tab">"Home"</A>
            <A href="/benchmark" class="nav-tab">"Benchmark"</A>
            <A href="/stress" class="nav-tab">"Stress Test"</A>
            <A href="/eval" class="nav-tab">"Eval"</A>
        </nav>
    }
}
