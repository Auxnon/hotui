use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

use nvml_wrapper::Nvml;
use nvml_wrapper::enum_wrappers::device::TemperatureSensor;

use crate::config::{SensorConfig, Source};

const HWMON_ROOT: &str = "/sys/class/hwmon";

enum Reader {
    File(PathBuf),
    Nvidia(u32),
    Missing(String),
}

pub struct Sensor {
    pub cfg: SensorConfig,
    reader: Reader,
    /// (seconds since start, °C)
    pub history: VecDeque<(f64, f64)>,
    pub current: Option<f64>,
    pub peak: Option<f64>,
    pub low: Option<f64>,
    /// Extra one-line info shown in the title (e.g. GPU fan/power).
    pub extra: Option<String>,
    pub error: Option<String>,
}

impl Sensor {
    pub fn new(cfg: SensorConfig, nvml: Option<&Nvml>) -> Self {
        let reader = match &cfg.source {
            Source::Hwmon {
                chip,
                sensor,
                input,
            } => find_hwmon(chip, sensor.as_deref(), input.as_deref()),
            Source::Nvidia { index } => match nvml {
                Some(n) if n.device_by_index(*index).is_ok() => Reader::Nvidia(*index),
                Some(_) => Reader::Missing(format!("no NVIDIA GPU at index {index}")),
                None => Reader::Missing("NVML unavailable".into()),
            },
        };
        let error = match &reader {
            Reader::Missing(e) => Some(e.clone()),
            _ => None,
        };
        Self {
            cfg,
            reader,
            history: VecDeque::new(),
            current: None,
            peak: None,
            low: None,
            extra: None,
            error,
        }
    }

    pub fn sample(&mut self, t: f64, window: f64, nvml: Option<&Nvml>) {
        let value = match &self.reader {
            Reader::File(p) => fs::read_to_string(p)
                .map_err(|e| e.to_string())
                .and_then(|s| s.trim().parse::<f64>().map_err(|e| e.to_string()))
                .map(|m| m / 1000.0),
            Reader::Nvidia(i) => read_nvidia(nvml, *i).map(|(temp, extra)| {
                self.extra = extra;
                temp
            }),
            Reader::Missing(_) => return,
        };
        match value {
            Ok(v) => {
                self.error = None;
                self.current = Some(v);
                self.peak = Some(self.peak.map_or(v, |p| p.max(v)));
                self.low = Some(self.low.map_or(v, |l| l.min(v)));
                self.history.push_back((t, v));
            }
            Err(e) => {
                self.error = Some(e);
                self.current = None;
            }
        }
        while self.history.front().is_some_and(|&(ht, _)| ht < t - window) {
            self.history.pop_front();
        }
    }
}

fn read_nvidia(nvml: Option<&Nvml>, index: u32) -> Result<(f64, Option<String>), String> {
    let nvml = nvml.ok_or("NVML unavailable")?;
    let dev = nvml.device_by_index(index).map_err(|e| e.to_string())?;
    let temp = dev
        .temperature(TemperatureSensor::Gpu)
        .map_err(|e| e.to_string())?;
    let mut parts = Vec::new();
    if let Ok(fan) = dev.fan_speed(0) {
        parts.push(format!("fan {fan}%"));
    }
    if let Ok(mw) = dev.power_usage() {
        parts.push(format!("{:.0}W", mw as f64 / 1000.0));
    }
    let extra = (!parts.is_empty()).then(|| parts.join(" "));
    Ok((temp as f64, extra))
}

fn find_hwmon(chip: &str, label: Option<&str>, input: Option<&str>) -> Reader {
    let Ok(entries) = fs::read_dir(HWMON_ROOT) else {
        return Reader::Missing(format!("{HWMON_ROOT} not readable"));
    };
    let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    dirs.sort();
    for dir in dirs {
        if read_trim(&dir.join("name")).as_deref() != Some(chip) {
            continue;
        }
        if let Some(input) = input {
            let p = dir.join(input);
            if p.exists() {
                return Reader::File(p);
            }
        } else if let Some(label) = label
            && let Some(p) = find_by_label(&dir, label)
        {
            return Reader::File(p);
        }
    }
    let what = input.or(label).unwrap_or("?");
    Reader::Missing(format!("hwmon {chip}/{what} not found"))
}

fn find_by_label(dir: &Path, label: &str) -> Option<PathBuf> {
    fs::read_dir(dir).ok()?.flatten().find_map(|e| {
        let name = e.file_name().into_string().ok()?;
        let prefix = name.strip_suffix("_label")?;
        (read_trim(&e.path()).as_deref() == Some(label))
            .then(|| dir.join(format!("{prefix}_input")))
    })
}

fn read_trim(p: &Path) -> Option<String> {
    fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

/// "Board · CPU · GPU" line for the header.
pub fn system_summary(nvml: Option<&Nvml>) -> String {
    let board = read_trim(Path::new("/sys/class/dmi/id/board_name"));
    let cpu = fs::read_to_string("/proc/cpuinfo").ok().and_then(|s| {
        s.lines()
            .find(|l| l.starts_with("model name"))
            .and_then(|l| l.split(':').nth(1))
            .map(|m| m.trim().to_string())
    });
    let gpu = nvml
        .and_then(|n| n.device_by_index(0).ok())
        .and_then(|d| d.name().ok());
    [board, cpu, gpu]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
}
