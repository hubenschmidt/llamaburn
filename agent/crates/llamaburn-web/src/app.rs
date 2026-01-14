use leptos::prelude::*;
use leptos_router::components::*;
use leptos_router::path;

use crate::components::header::Header;
use crate::components::nav::NavTabs;
use crate::pages::{benchmark::BenchmarkPage, eval::EvalPage, home::HomePage, stress::StressPage};

#[component]
pub fn App() -> impl IntoView {
    view! {
        <Router>
            <div class="app">
                <Header />
                <NavTabs />
                <main class="content">
                    <Routes fallback=|| view! { <p>"Page not found"</p> }>
                        <Route path=path!("/") view=HomePage />
                        <Route path=path!("/benchmark") view=BenchmarkPage />
                        <Route path=path!("/stress") view=StressPage />
                        <Route path=path!("/eval") view=EvalPage />
                    </Routes>
                </main>
            </div>
        </Router>
    }
}
