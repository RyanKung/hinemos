use hinemos_core::{Direction, EntityKind, JsonObservation};

use super::visual_width;

pub(super) fn apply_auto_ascii_map(observation: &mut JsonObservation, width: usize) {
    if !observation.ascii_art.is_empty() {
        observation.ascii_art = normalize_map_lines(observation.ascii_art.clone(), width);
        return;
    }
    let lines = match observation.view_id.as_str() {
        "arrival_street" => harbor_square_map(observation, width),
        "west_main_street" => hinemos_blvd_map(
            "WEST HINEMOS BLVD",
            "wilderness",
            "Harbor Square",
            Some("H1"),
            Some("H2"),
            width,
        ),
        "official_street" => hinemos_blvd_map(
            "EAST HINEMOS BLVD",
            "Harbor Square",
            "wilderness",
            Some("H3"),
            Some("H4"),
            width,
        ),
        view if view.starts_with("street_north_") || view.starts_with("street_south_") => {
            if observation.ascii_art.is_empty() {
                let (west, east) = inferred_agentopia_lots(view);
                agentopia_segment_map(view, &west, &east, width)
            } else {
                observation.ascii_art.clone()
            }
        }
        view if view.starts_with("parcel_") => parcel_map(observation),
        _ => linear_or_room_map(observation),
    };
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

fn harbor_square_map(observation: &JsonObservation, width: usize) -> Vec<String> {
    let mut canvas = MapCanvas::new(width, 13);
    let cx = width / 2;
    canvas.centered_text(0, "MAP");
    canvas.centered_text(1, "north");
    canvas.vroad(cx, 2, 10);
    canvas.hroad(5, 2, width.saturating_sub(3));
    canvas.text_centered(cx, 5, "[<Me>]");
    canvas.centered_text(11, "south");
    let mut lines = canvas.render();
    lines.push(route_line(observation));
    lines.push(item_line(observation));
    lines.into_iter().filter(|line| !line.is_empty()).collect()
}

fn hinemos_blvd_map(
    title: &str,
    west: &str,
    east: &str,
    north: Option<&str>,
    south: Option<&str>,
    width: usize,
) -> Vec<String> {
    let mut canvas = MapCanvas::new(width, 15);
    let cx = width / 2;
    canvas.centered_text(0, &format!("MAP: {title}"));
    canvas.shop(cx, 1, north.unwrap_or("-"));
    canvas.text_centered(cx, 6, "│");
    canvas.hroad(8, 2, width.saturating_sub(3));
    canvas.text_centered(cx, 8, "[<Me>]");
    canvas.text_centered(cx, 10, "│");
    canvas.shop(cx, 11, south.unwrap_or("-"));
    let mut lines = canvas.render();
    lines.push(format!("Routes: west -> {west}; east -> {east}"));
    lines
}

fn agentopia_segment_map(view_id: &str, west: &str, east: &str, width: usize) -> Vec<String> {
    let north = agentopia_axis_label(view_id, Direction::North);
    let south = agentopia_axis_label(view_id, Direction::South);
    let mut canvas = MapCanvas::new(width, 11);
    let cx = width / 2;
    let west_x = cx.saturating_sub(width / 4);
    let east_x = (cx + width / 4).min(width.saturating_sub(4));
    canvas.centered_text(0, "MAP: AGENTOPIA BLVD");
    canvas.centered_text(1, &format!("north: {north}"));
    canvas.vroad(cx, 2, 8);
    canvas.lot(west_x, 4, west);
    canvas.lot(east_x, 4, east);
    canvas.text_centered(cx, 5, "<Me>");
    canvas.centered_text(9, &format!("south: {south}"));
    canvas.render()
}

fn parcel_map(observation: &JsonObservation) -> Vec<String> {
    let exit = observation.exits.first();
    let mut lines = vec!["MAP: LOT".to_owned()];
    lines.extend(lot_icon());
    lines.extend([
        "                    [<Me>]".to_owned(),
        format!(
            "exit {}: {}",
            exit.map(|exit| exit.direction.as_str()).unwrap_or("back"),
            exit.and_then(|exit| exit.label.as_deref())
                .unwrap_or("Agentopia Blvd")
        ),
    ]);
    lines
}

fn linear_or_room_map(observation: &JsonObservation) -> Vec<String> {
    let west = exit_label(observation, Direction::West);
    let east = exit_label(observation, Direction::East);
    if west != "-" || east != "-" {
        if observation.title.to_ascii_lowercase().contains("sea") {
            return sea_map(observation);
        }
        return vec![
            "MAP".to_owned(),
            "  __________________________________________".to_owned(),
            "  _ _ _ _ _ _ _ _ [<Me>] _ _ _ _ _ _ _ _".to_owned(),
            "  __________________________________________".to_owned(),
            format!("Routes: west -> {west}; east -> {east}"),
        ];
    }
    if observation.title.to_ascii_lowercase().contains("sea") {
        return sea_map(observation);
    }
    vec!["MAP".to_owned(), "[<Me>]".to_owned()]
}

fn lot_icon() -> Vec<String> {
    vec![
        "┌────┐".to_owned(),
        "│    │".to_owned(),
        "└────┘".to_owned(),
    ]
}

fn sea_map(observation: &JsonObservation) -> Vec<String> {
    let mut lines = vec![
        "MAP: SEA".to_owned(),
        "≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈".to_owned(),
        "≈≈≈≈≈≈≈≈≈≈≈[<Me>]≈≈≈≈≈≈≈".to_owned(),
        "≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈".to_owned(),
    ];
    for exit in &observation.exits {
        lines.push(format!(
            "shore {}: {}",
            exit.direction.as_str(),
            exit.label.as_deref().unwrap_or("beach")
        ));
    }
    lines
}

fn short_label(label: &str, max: usize) -> String {
    let mut output = label.chars().take(max).collect::<String>();
    if label.chars().count() > max && max > 1 {
        output.pop();
        output.push('…');
    }
    output
}

fn exit_label(observation: &JsonObservation, direction: Direction) -> String {
    observation
        .exits
        .iter()
        .find(|exit| exit.direction == direction)
        .and_then(|exit| exit.label.as_deref())
        .unwrap_or("-")
        .to_owned()
}

fn route_line(observation: &JsonObservation) -> String {
    let routes = observation
        .exits
        .iter()
        .map(|exit| {
            format!(
                "{} -> {}",
                exit.direction.as_str(),
                short_label(exit.label.as_deref().unwrap_or("-"), 14)
            )
        })
        .collect::<Vec<_>>();
    if routes.is_empty() {
        String::new()
    } else {
        format!("Routes: {}", routes.join("; "))
    }
}

struct MapCanvas {
    width: usize,
    rows: Vec<Vec<char>>,
}

impl MapCanvas {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            rows: vec![vec![' '; width]; height],
        }
    }

    fn render(self) -> Vec<String> {
        self.rows
            .into_iter()
            .map(|row| row.into_iter().collect::<String>())
            .collect()
    }

    fn put(&mut self, x: usize, y: usize, ch: char) {
        if y < self.rows.len() && x < self.width {
            self.rows[y][x] = ch;
        }
    }

    fn text(&mut self, x: usize, y: usize, text: &str) {
        for (offset, ch) in text.chars().enumerate() {
            self.put(x + offset, y, ch);
        }
    }

    fn text_centered(&mut self, x: usize, y: usize, text: &str) {
        let start = x.saturating_sub(text.chars().count() / 2);
        self.text(start, y, text);
    }

    fn centered_text(&mut self, y: usize, text: &str) {
        self.text_centered(self.width / 2, y, text);
    }

    fn hroad(&mut self, y: usize, x1: usize, x2: usize) {
        if x2 <= x1 + 2 {
            return;
        }
        for x in x1..=x2 {
            self.put(x, y.saturating_sub(1), '_');
            self.put(x, y + 1, '_');
            self.put(x, y, if (x - x1).is_multiple_of(2) { '_' } else { ' ' });
        }
    }

    fn vroad(&mut self, x: usize, y1: usize, y2: usize) {
        if x == 0 || x + 2 >= self.width {
            return;
        }
        for y in y1..=y2 {
            self.put(x - 2, y, '│');
            self.put(
                x,
                y,
                if (y - y1).is_multiple_of(2) {
                    '┆'
                } else {
                    ' '
                },
            );
            self.put(x + 2, y, '│');
        }
    }

    fn lot(&mut self, x: usize, y: usize, label: &str) {
        self.text(x, y, "┌────┐");
        self.text(x, y + 1, &format!("│ {:<3}│", short_label(label, 3)));
        self.text(x, y + 2, "└────┘");
    }

    fn shop(&mut self, x: usize, y: usize, label: &str) {
        let left = x.saturating_sub(8);
        self.text(left, y, " /^^^^^^^^^^^^\\");
        self.text(left, y + 1, "/______________\\");
        self.text(left, y + 2, &format!("| {:<12} |", short_label(label, 12)));
        self.text(left, y + 3, "|______________|");
    }
}

fn item_line(observation: &JsonObservation) -> String {
    let names = observation
        .entities
        .iter()
        .filter(|entity| matches!(entity.kind, EntityKind::Item | EntityKind::Object))
        .map(|entity| entity.name.as_str())
        .collect::<Vec<_>>();
    if names.is_empty() {
        String::new()
    } else {
        format!("objects: {}", names.join(", "))
    }
}

fn inferred_agentopia_lots(view_id: &str) -> (String, String) {
    let Some(segment) = view_id
        .rsplit('_')
        .next()
        .and_then(|value| value.parse::<i32>().ok())
    else {
        return ("west lot".to_owned(), "east lot".to_owned());
    };
    let prefix = if view_id.contains("_north_") {
        "N"
    } else {
        "S"
    };
    let west = segment * 2 - 1;
    let east = segment * 2;
    (format!("{prefix}{west}"), format!("{prefix}{east}"))
}

fn agentopia_axis_label(view_id: &str, direction: Direction) -> String {
    let Some(segment) = view_id
        .rsplit('_')
        .next()
        .and_then(|value| value.parse::<i32>().ok())
    else {
        return "-".to_owned();
    };
    let north_side = view_id.contains("_north_");
    match (north_side, direction) {
        (true, Direction::North) if segment < 5 => format!("segment {:02}", segment + 1),
        (true, Direction::North) => "north edge".to_owned(),
        (true, Direction::South) if segment == 1 => "Harbor Square".to_owned(),
        (true, Direction::South) => format!("segment {:02}", segment - 1),
        (false, Direction::North) if segment == 1 => "Harbor Square".to_owned(),
        (false, Direction::North) => format!("segment {:02}", segment - 1),
        (false, Direction::South) if segment < 5 => format!("segment {:02}", segment + 1),
        (false, Direction::South) => "south edge".to_owned(),
        _ => "-".to_owned(),
    }
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
