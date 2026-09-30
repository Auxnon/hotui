use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, BorderType, Chart, Dataset, GraphType, Paragraph};

use crate::App;
use crate::sensors::Sensor;

const OVER: Color = Color::Red;
const LINE: Color = Color::Cyan;
const THRESHOLD: Color = Color::DarkGray;

pub fn draw(frame: &mut Frame, app: &App) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    let over = app.sensors.iter().filter(|s| is_over(s)).count();
    let mut head = vec![
        " hotui ".black().on_cyan().bold(),
        Span::raw(format!("  {}", app.summary)),
    ];
    if over > 0 {
        head.push(format!("   ▲ {over} over threshold").red().bold());
    }
    if app.paused {
        head.push("   ⏸ paused".yellow().bold());
    }
    frame.render_widget(Line::from(head), header);

    let status = app.status.as_deref().unwrap_or("");
    frame.render_widget(
        Line::from(vec![
            " q".bold(),
            " quit  ".dark_gray(),
            "p".bold(),
            " pause  ".dark_gray(),
            "r".bold(),
            " reload config  ".dark_gray(),
            "+/-".bold(),
            " columns  ".dark_gray(),
            Span::raw(format!("{}  ", app.config_path.display())).dark_gray(),
            status.yellow(),
        ]),
        footer,
    );

    let sensors: Vec<&Sensor> = app.sensors.iter().collect();
    if sensors.is_empty() {
        frame.render_widget(
            Paragraph::new("No enabled sensors in config — press r after editing it."),
            body,
        );
        return;
    }
    let cols = app.columns.max(1) as usize;
    let rows = sensors.len().div_ceil(cols);
    let row_areas = Layout::vertical(vec![Constraint::Ratio(1, rows as u32); rows]).split(body);
    for (r, chunk) in sensors.chunks(cols).enumerate() {
        let cells =
            Layout::horizontal(vec![Constraint::Ratio(1, cols as u32); cols]).split(row_areas[r]);
        for (c, sensor) in chunk.iter().enumerate() {
            draw_sensor(frame, cells[c], sensor, app.now, app.window);
        }
    }
}

fn is_over(s: &Sensor) -> bool {
    s.current.is_some_and(|v| v > s.cfg.max)
}

fn level_color(s: &Sensor, v: f64) -> Color {
    let frac = (v - s.cfg.min) / (s.cfg.max - s.cfg.min);
    match frac {
        f if f > 1.0 => OVER,
        f if f > 0.85 => Color::LightRed,
        f if f > 0.6 => Color::Yellow,
        _ => Color::Green,
    }
}

fn draw_sensor(frame: &mut Frame, area: Rect, s: &Sensor, now: f64, window: f64) {
    let over = is_over(s);
    let mut title = vec![Span::raw(" "), s.cfg.label.clone().bold(), Span::raw(" ")];
    if let Some(v) = s.current {
        title.push(Span::styled(
            format!("{v:.1}°C "),
            Style::new()
                .fg(level_color(s, v))
                .add_modifier(Modifier::BOLD),
        ));
    }
    let mut stats = Vec::new();
    if let (Some(lo), Some(hi)) = (s.low, s.peak) {
        stats.push(Span::raw(format!(" lo {lo:.0} ")).dark_gray());
        let peak_style = if hi > s.cfg.max {
            Style::new().fg(OVER)
        } else {
            Style::new().dark_gray()
        };
        stats.push(Span::styled(format!("hi {hi:.0} "), peak_style));
    }
    if s.peak.is_some_and(|p| p > s.cfg.max) {
        stats.push(Span::raw(format!("max {:.0} ", s.cfg.max)).fg(OVER));
    }
    if let Some(extra) = &s.extra {
        stats.push(Span::raw(format!("· {extra} ")).dark_gray());
    }
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(if over {
            Style::new().fg(OVER)
        } else {
            Style::new().dark_gray()
        })
        .title(Line::from(title))
        .title(Line::from(stats).right_aligned());

    if let Some(err) = &s.error {
        frame.render_widget(Paragraph::new(err.as_str().red()).block(block), area);
        return;
    }

    let data: Vec<(f64, f64)> = s.history.iter().map(|&(t, v)| (t - now, v)).collect();
    let (cfg_lo, cfg_hi) = (s.cfg.min, s.cfg.max);
    let data_lo = data.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let data_hi = data.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    // Rescale when a reading escapes the configured range, snapping to 5°C.
    let expanded_hi = data_hi > cfg_hi;
    let lo = if data_lo < cfg_lo {
        (data_lo / 5.0).floor() * 5.0
    } else {
        cfg_lo
    };
    let hi = if expanded_hi {
        (data_hi / 5.0).ceil() * 5.0
    } else {
        cfg_hi
    };
    let hi = if hi <= data_hi && expanded_hi {
        hi + 5.0
    } else {
        hi
    };

    let over_runs = over_segments(&data, cfg_hi);
    let over_points: Vec<(f64, f64)> = data.iter().copied().filter(|p| p.1 > cfg_hi).collect();
    let threshold = [(-window, cfg_hi), (0.0, cfg_hi)];

    let mut datasets = Vec::new();
    if expanded_hi {
        datasets.push(
            Dataset::default()
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::new().fg(THRESHOLD))
                .data(&threshold),
        );
    }
    datasets.push(
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::new().fg(LINE))
            .data(&data),
    );
    for run in &over_runs {
        datasets.push(
            Dataset::default()
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::new().fg(OVER))
                .data(run),
        );
    }
    datasets.push(
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Scatter)
            .style(Style::new().fg(OVER))
            .data(&over_points),
    );

    let hi_label = if expanded_hi {
        format!("{hi:.0}").red().bold()
    } else {
        format!("{hi:.0}").into()
    };
    let y_labels: Vec<Line> = vec![
        format!("{lo:.0}").into(),
        format!("{:.0}", (lo + hi) / 2.0).into(),
        Line::from(hi_label),
    ];
    let chart = Chart::new(datasets)
        .block(block)
        .x_axis(
            Axis::default()
                .bounds([-window, 0.0])
                .style(Style::new().dark_gray())
                .labels(vec![
                    Line::from(format!("-{window:.0}s")),
                    Line::from(format!("-{:.0}s", window / 2.0)),
                    Line::from("now"),
                ]),
        )
        .y_axis(
            Axis::default()
                .bounds([lo, hi])
                .style(Style::new().dark_gray())
                .labels(y_labels),
        );
    frame.render_widget(chart, area);
}

/// Split the samples above `limit` into contiguous runs, starting and ending
/// each run at the interpolated crossing point so the red line meets the
/// threshold exactly.
fn over_segments(data: &[(f64, f64)], limit: f64) -> Vec<Vec<(f64, f64)>> {
    let cross = |a: (f64, f64), b: (f64, f64)| {
        let f = (limit - a.1) / (b.1 - a.1);
        (a.0 + f * (b.0 - a.0), limit)
    };
    let mut runs = Vec::new();
    let mut cur: Vec<(f64, f64)> = Vec::new();
    for (i, &p) in data.iter().enumerate() {
        let prev = i.checked_sub(1).map(|j| data[j]);
        if p.1 > limit {
            if cur.is_empty()
                && let Some(a) = prev
            {
                cur.push(cross(a, p));
            }
            cur.push(p);
        } else if !cur.is_empty() {
            if let Some(a) = prev {
                cur.push(cross(a, p));
            }
            runs.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        runs.push(cur);
    }
    runs
}
