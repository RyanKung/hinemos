use hinemos_core::JsonObservation;

use super::visual_width;

pub(super) fn apply_auto_ascii_map(
    observation: &mut JsonObservation,
    width: usize,
    _admission_view_id: &str,
) {
    let lines = std::mem::take(&mut observation.ascii_art);
    observation.ascii_art = normalize_map_lines(lines, width);
}

fn normalize_map_lines(lines: Vec<String>, width: usize) -> Vec<String> {
    lines
        .into_iter()
        .flat_map(|line| line.split('\n').map(str::to_owned).collect::<Vec<_>>())
        .map(|line| fit_map_line(&line, width))
        .collect()
}

fn fit_map_line(line: &str, width: usize) -> String {
    let mut fitted = line.chars().take(width).collect::<String>();
    if line.chars().count() > width && width > 1 {
        fitted.pop();
        fitted.push('…');
    }
    let padding = width.saturating_sub(visual_width(&fitted));
    fitted.push_str(&" ".repeat(padding));
    fitted
}

pub(super) fn overlay_ascii_title(observation: &mut JsonObservation, title: &str) {
    let old_title = observation.title.trim().to_ascii_uppercase();
    if old_title.is_empty() {
        return;
    }

    for line in &mut observation.ascii_art {
        if line.trim() == old_title {
            let indent = line.len() - line.trim_start().len();
            *line = format!("{}[{title}]", " ".repeat(indent));
        }
    }
}

pub(super) fn overlay_ascii_parcel_label(
    observation: &mut JsonObservation,
    parcel_id: &str,
    title: &str,
) {
    let from = format!("[{parcel_id}]");
    let to = format!("[{title}]");
    for line in &mut observation.ascii_art {
        if line.contains(&from) {
            *line = line.replace(&from, &to);
        }
    }
}
