use eframe::egui;
use llamaburn_services::AppModels;
use llamaburn_services::Services;

use crate::panels::{
    benchmark::BenchmarkPanel, gpu_monitor::GpuMonitorPanel,
    history::{HistoryPanel, LoadCodeBenchmarkRequest},
    setup::SetupPanel,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Home,
    Benchmark,
    Stress,
    Eval,
    History,
    Docs,
    Setup,
}

pub struct LlamaBurnApp {
    current_tab: Tab,

    // Models and Services (owned directly)
    app_models: AppModels,
    services: Services,

    // Panels (views)
    gpu_monitor: GpuMonitorPanel,
    benchmark: BenchmarkPanel,
    history: HistoryPanel,
    setup: SetupPanel,
}

impl LlamaBurnApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Create services (owned directly)
        let services = Services::new();

        // Create models (owned directly)
        let mut app_models = AppModels::new();

        // Start loading models
        services.start_loading_models(&mut app_models);

        // Create benchmark panel (services passed via ui())
        let benchmark = BenchmarkPanel::new(&services);

        Self {
            current_tab: Tab::Home,
            app_models,
            gpu_monitor: GpuMonitorPanel::new(),
            benchmark,
            history: HistoryPanel::new(services.history.clone()),
            setup: SetupPanel::new(services.history.clone()),
            services,
        }
    }

    fn render_tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.current_tab, Tab::Home, "Home");
            ui.selectable_value(&mut self.current_tab, Tab::Benchmark, "Benchmark");
            ui.selectable_value(&mut self.current_tab, Tab::Stress, "Stress Test");
            ui.selectable_value(&mut self.current_tab, Tab::Eval, "Eval");
            ui.selectable_value(&mut self.current_tab, Tab::History, "History");
            ui.selectable_value(&mut self.current_tab, Tab::Docs, "Docs");
            ui.selectable_value(&mut self.current_tab, Tab::Setup, "Setup");
        });
    }

    fn render_home(&self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("LlamaBurn").heading().color(egui::Color32::from_rgb(255, 69, 0)));
        ui.add_space(10.0);
        ui.label("GPU Benchmark & Stress Testing Tool");
        ui.add_space(20.0);

        ui.group(|ui| {
            ui.label("Benchmark");
            ui.label("Run inference benchmarks to measure tokens per second.");
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.label("Stress Test");
            ui.label("Push your GPU to its limits with sustained workloads.");
        });

        ui.add_space(10.0);

        ui.group(|ui| {
            ui.label("Eval");
            ui.label("Evaluate model outputs for quality and correctness.");
        });
    }

    fn render_stress(&self, ui: &mut egui::Ui) {
        ui.heading("Stress Test");
        ui.label("Coming soon...");
    }

    fn render_eval(&self, ui: &mut egui::Ui) {
        ui.heading("Eval");
        ui.label("Coming soon...");
    }

    fn render_docs(&self, ui: &mut egui::Ui) {
        ui.heading("Documentation");
        ui.add_space(10.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label("CLI Usage:");
            ui.code("llamaburn benchmark --model <model> --iterations 10");
            ui.add_space(10.0);
            ui.label("Options:");
            ui.code("  --model, -m       Model name to benchmark");
            ui.code("  --iterations, -i  Number of iterations (default: 10)");
            ui.code("  --warmup, -w      Warmup iterations (default: 2)");
            ui.code("  --temperature, -t Temperature (default: 0.7)");
        });
    }
}

impl eframe::App for LlamaBurnApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for load request from history panel
        if let Some(req) = self.history.take_load_request() {
            self.handle_load_request(req);
        }

        egui::SidePanel::right("gpu_panel")
            .default_width(420.0)
            .show(ctx, |ui| {
                self.gpu_monitor.ui(ui);
            });

        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            self.render_tabs(ui);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.current_tab {
                Tab::Home => self.render_home(ui),
                Tab::Benchmark => self.benchmark.ui(ui, &mut self.app_models, &self.services),
                Tab::Stress => self.render_stress(ui),
                Tab::Eval => self.render_eval(ui),
                Tab::History => self.history.ui(ui),
                Tab::Docs => self.render_docs(ui),
                Tab::Setup => self.setup.ui(ui),
            }
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

impl LlamaBurnApp {
    fn handle_load_request(&mut self, req: LoadCodeBenchmarkRequest) {
        // Switch to Benchmark tab and load params
        self.current_tab = Tab::Benchmark;
        self.benchmark.load_code_from_history(
            req.model_id,
            req.language,
            req.temperature,
            req.max_tokens,
            req.problem_ids,
        );
    }
}
