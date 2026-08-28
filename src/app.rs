use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::Terminal;

use crate::collectors::Collectors;
use crate::config::Config;
use crate::models::{
    GpuHistory, SystemHistory, InferenceHistory,
    GpuInfo, GpuMetrics, GpuProcess,
    SystemInfo, SystemMetrics, ProcessInfo,
    InferenceEngine, ModelInstance, InferenceMetrics,
};
use crate::ui::Ui;

pub struct App {
    pub config: Config,
    pub collectors: Collectors,
    pub ui: Ui,
    pub running: bool,
    pub current_view: View,
    pub selected_panel: usize,
    pub paused: bool,
    pub last_refresh: Instant,

    // GPU data
    pub gpu_info: Vec<GpuInfo>,
    pub gpu_metrics: Vec<GpuMetrics>,
    pub gpu_processes: Vec<GpuProcess>,
    pub gpu_history: Vec<GpuHistory>,

    // System data
    pub system_info: Option<SystemInfo>,
    pub system_metrics: Option<SystemMetrics>,
    pub system_processes: Vec<ProcessInfo>,
    pub system_history: SystemHistory,

    // Inference data
    pub inference_engines: Vec<InferenceEngine>,
    pub model_instances: Vec<ModelInstance>,
    pub inference_metrics: Vec<InferenceMetrics>,
    pub inference_history: InferenceHistory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Overview,
    Gpu,
    Llm,
    Processes,
    System,
    Network,
}

impl App {
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        let collectors = Collectors::new();
        let ui = Ui::new(config.clone());

        let max_points = 300;

        Ok(Self {
            config,
            collectors,
            ui,
            running: true,
            current_view: View::Overview,
            selected_panel: 0,
            paused: false,
            last_refresh: Instant::now(),
            gpu_info: Vec::new(),
            gpu_metrics: Vec::new(),
            gpu_processes: Vec::new(),
            gpu_history: Vec::new(),
            system_info: None,
            system_metrics: None,
            system_processes: Vec::new(),
            system_history: SystemHistory::new(max_points),
            inference_engines: Vec::new(),
            model_instances: Vec::new(),
            inference_metrics: Vec::new(),
            inference_history: InferenceHistory::new(max_points),
        })
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
        let mut terminal = Terminal::new(backend)?;

        self.init_terminal()?;

        loop {
            if !self.paused && self.last_refresh.elapsed() >= Duration::from_millis(self.config.refresh_ms) {
                self.refresh().await;
                self.last_refresh = Instant::now();
            }

            self.ui.render(&mut terminal, &self)?;

            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.handle_key(key);
                    }
                }
            }

            if !self.running {
                break;
            }
        }

        self.restore_terminal()?;
        Ok(())
    }

    async fn refresh(&mut self) {
        // Collect GPU data
        let gpu_data = self.collectors.nvml.collect();
        self.gpu_info.clear();
        self.gpu_metrics.clear();
        self.gpu_processes.clear();

        for (info, metrics, processes) in gpu_data {
            self.gpu_info.push(info);
            self.gpu_metrics.push(metrics);
            self.gpu_processes.extend(processes);
        }

        // Collect system data
        let (sys_info, sys_metrics, sys_procs) = self.collectors.system.collect();
        self.system_info = Some(sys_info);
        self.system_metrics = Some(sys_metrics);
        self.system_processes = sys_procs;

        // Collect inference data
        let inference_data = self.collectors.inference.collect().await;
        self.inference_engines.clear();
        self.model_instances.clear();
        self.inference_metrics.clear();

        for (engine, models, metrics) in inference_data {
            self.inference_engines.push(engine);
            self.model_instances.extend(models);
            self.inference_metrics.push(metrics);
        }
    }

    fn handle_key(&mut self, key: event::KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.running = false,
            KeyCode::Char('1') => self.current_view = View::Overview,
            KeyCode::Char('2') => self.current_view = View::Gpu,
            KeyCode::Char('3') => self.current_view = View::Llm,
            KeyCode::Char('4') => self.current_view = View::Processes,
            KeyCode::Char('5') => self.current_view = View::System,
            KeyCode::Char('6') => self.current_view = View::Network,
            KeyCode::Char('r') => {
                self.last_refresh = Instant::now();
            }
            KeyCode::Char('p') => self.paused = !self.paused,
            KeyCode::Tab => self.selected_panel = (self.selected_panel + 1) % 3,
            KeyCode::Up | KeyCode::Char('k') => {}
            KeyCode::Down | KeyCode::Char('j') => {}
            KeyCode::Left | KeyCode::Char('h') => {}
            KeyCode::Right | KeyCode::Char('l') => {}
            KeyCode::Enter => {}
            KeyCode::Char('s') => {}
            KeyCode::Char('?') => {}
            KeyCode::Esc => {}
            _ => {}
        }
    }

    fn init_terminal(&self) -> anyhow::Result<()> {
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen)?;
        Ok(())
    }

    fn restore_terminal(&self) -> anyhow::Result<()> {
        crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen)?;
        crossterm::terminal::disable_raw_mode()?;
        Ok(())
    }
}