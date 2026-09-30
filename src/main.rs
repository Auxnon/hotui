mod config;
mod sensors;
mod ui;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use nvml_wrapper::Nvml;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use config::Config;
use sensors::Sensor;

pub struct App {
    pub sensors: Vec<Sensor>,
    pub summary: String,
    pub config_path: PathBuf,
    pub columns: u16,
    pub window: f64,
    pub now: f64,
    pub paused: bool,
    pub status: Option<String>,
    refresh: Duration,
}

impl App {
    fn new(cfg: Config, config_path: PathBuf, nvml: Option<&Nvml>) -> Self {
        let mut app = Self {
            sensors: Vec::new(),
            summary: sensors::system_summary(nvml),
            config_path,
            columns: 1,
            window: 0.0,
            now: 0.0,
            paused: false,
            status: None,
            refresh: Duration::ZERO,
        };
        app.apply(cfg, nvml);
        app
    }

    fn apply(&mut self, cfg: Config, nvml: Option<&Nvml>) {
        self.columns = cfg.general.columns.max(1);
        self.window = cfg.general.history_secs.max(5) as f64;
        self.refresh = Duration::from_millis(cfg.general.refresh_ms.max(100));
        self.sensors = cfg
            .sensors
            .into_iter()
            .filter(|s| s.enabled)
            .map(|s| Sensor::new(s, nvml))
            .collect();
    }

    fn sample(&mut self, t: f64, nvml: Option<&Nvml>) {
        self.now = t;
        for s in &mut self.sensors {
            s.sample(t, self.window, nvml);
        }
    }
}

fn parse_args() -> Result<Option<PathBuf>> {
    let mut args = std::env::args().skip(1);
    let mut path = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "-c" | "--config" => {
                path = Some(PathBuf::from(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("--config needs a path"))?,
                ))
            }
            "--print-default-config" => {
                print!("{}", config::DEFAULT_CONFIG);
                std::process::exit(0);
            }
            "-h" | "--help" => {
                println!(
                    "hotui — live temperature graphs\n\n\
                     usage: hotui [-c|--config PATH] [--print-default-config]\n\n\
                     Default config: ~/.config/hotui/config.toml (created on first run)"
                );
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown argument: {other} (try --help)"),
        }
    }
    Ok(path)
}

fn main() -> Result<()> {
    let config_path = config::resolve_path(parse_args()?)?;
    let cfg = config::load(&config_path)?;
    let nvml = Nvml::init().ok();

    let mut app = App::new(cfg, config_path, nvml.as_ref());
    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app, nvml.as_ref());
    ratatui::restore();
    result
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App, nvml: Option<&Nvml>) -> Result<()> {
    let start = Instant::now();
    app.sample(0.0, nvml);
    let mut next = Instant::now() + app.refresh;
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        let timeout = next.saturating_duration_since(Instant::now());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(());
                    }
                    KeyCode::Char('p') | KeyCode::Char(' ') => app.paused = !app.paused,
                    KeyCode::Char('+') | KeyCode::Char('=') => {
                        app.columns = (app.columns + 1).min(8)
                    }
                    KeyCode::Char('-') => app.columns = app.columns.saturating_sub(1).max(1),
                    KeyCode::Char('r') => match config::load(&app.config_path) {
                        Ok(cfg) => {
                            app.apply(cfg, nvml);
                            app.sample(start.elapsed().as_secs_f64(), nvml);
                            app.status = Some("config reloaded".into());
                        }
                        Err(e) => app.status = Some(format!("reload failed: {e:#}")),
                    },
                    _ => {}
                }
            }
            continue;
        }

        next += app.refresh;
        if Instant::now() > next {
            next = Instant::now() + app.refresh;
        }
        if !app.paused {
            app.sample(start.elapsed().as_secs_f64(), nvml);
        }
    }
}
