use leptos::prelude::*;
use wasm_bindgen::{closure::Closure, JsCast};
use web_sys::{EventSource, MessageEvent};

#[component]
pub fn GpuMonitor() -> impl IntoView {
    let (gpu_data, set_gpu_data) = signal(String::from("Connecting to GPU monitor..."));
    let (connected, set_connected) = signal(false);

    Effect::new(move || {
        let es = EventSource::new("/api/gpu/stream").ok();

        if let Some(event_source) = es {
            set_connected.set(true);

            let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
                if let Some(data) = e.data().as_string() {
                    set_gpu_data.set(data);
                }
            }) as Box<dyn FnMut(MessageEvent)>);

            event_source.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
            onmessage.forget();

            let onerror = Closure::wrap(Box::new(move |_| {
                set_connected.set(false);
                set_gpu_data.set("GPU monitor disconnected".to_string());
            }) as Box<dyn FnMut(web_sys::Event)>);

            event_source.set_onerror(Some(onerror.as_ref().unchecked_ref()));
            onerror.forget();
        }
    });

    view! {
        <div class="gpu-monitor">
            <div class="gpu-monitor-header">
                <span class="gpu-title">"GPU Monitor"</span>
                <span class={move || if connected.get() { "status-dot connected" } else { "status-dot disconnected" }}></span>
            </div>
            <pre class="gpu-output">{gpu_data}</pre>
        </div>
    }
}
