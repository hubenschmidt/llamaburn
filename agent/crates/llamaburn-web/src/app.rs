use leptos::prelude::*;
use leptos_router::components::*;
use leptos_router::path;

use crate::components::gpu_monitor::GpuMonitor;
use crate::components::header::Header;
use crate::components::nav::NavTabs;
use crate::pages::{benchmark::BenchmarkPage, docs::DocsPage, eval::EvalPage, home::HomePage, stress::StressPage};

#[component]
pub fn App() -> impl IntoView {
    view! {
        <Router>
            <div class="app">
                <Header />
                <NavTabs />
                <div class="app-body">
                    <main class="content">
                        <Routes fallback=|| view! { <p>"Page not found"</p> }>
                            <Route path=path!("/") view=HomePage />
                            <Route path=path!("/benchmark") view=BenchmarkPage />
                            <Route path=path!("/stress") view=StressPage />
                            <Route path=path!("/eval") view=EvalPage />
                            <Route path=path!("/docs") view=DocsPage />
                        </Routes>
                    </main>
                    <aside class="gpu-sidebar">
                        <GpuMonitor />
                    </aside>
                </div>
            </div>
        </Router>
    }
}
