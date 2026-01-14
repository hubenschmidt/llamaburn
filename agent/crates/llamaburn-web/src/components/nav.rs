use leptos::prelude::*;
use leptos_router::components::A;

#[component]
pub fn NavTabs() -> impl IntoView {
    view! {
        <nav class="nav-tabs">
            <A href="/" attr:class="nav-tab">"Home"</A>
            <A href="/benchmark" attr:class="nav-tab">"Benchmark"</A>
            <A href="/stress" attr:class="nav-tab">"Stress Test"</A>
            <A href="/eval" attr:class="nav-tab">"Eval"</A>
            <A href="/docs" attr:class="nav-tab">"Docs"</A>
        </nav>
    }
}
