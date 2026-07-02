//! Infinite grid-road and building-address model.

use crate::command::Direction;
use crate::model::{DEFAULT_ADMISSION_VIEW_ID, Exit, View};

/// Prefix for generated road view ids.
pub const GRID_ROAD_VIEW_PREFIX: &str = "grid_road_";
/// Prefix for generated building view ids.
pub const GRID_PARCEL_VIEW_PREFIX: &str = "parcel_";

const DEFAULT_GRID_ORIGIN_LABEL: &str = "Harbor Square";

/// Static view that anchors generated grid coordinates back into authored world content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridOrigin<'a> {
    view_id: &'a str,
    label: &'a str,
}

impl<'a> GridOrigin<'a> {
    /// Creates a generated-grid origin.
    #[must_use]
    pub const fn new(view_id: &'a str, label: &'a str) -> Self {
        Self { view_id, label }
    }

    /// Default sample-world grid origin.
    #[must_use]
    pub const fn default_admission() -> Self {
        Self::new(DEFAULT_ADMISSION_VIEW_ID, DEFAULT_GRID_ORIGIN_LABEL)
    }

    /// Static view id used when generated roads return to coordinate zero.
    #[must_use]
    pub const fn view_id(self) -> &'a str {
        self.view_id
    }

    /// Player-facing label for the static origin view.
    #[must_use]
    pub const fn label(self) -> &'a str {
        self.label
    }
}

/// Coordinate of a generated road crossing in the infinite town grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GridRoad {
    x: i32,
    y: i32,
}

impl GridRoad {
    /// Creates a generated grid-road coordinate.
    ///
    /// Returns `None` for coordinate zero because the plaza origin is represented by a static
    /// world view rather than a generated road.
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Option<Self> {
        if x == 0 && y == 0 {
            None
        } else {
            Some(Self { x, y })
        }
    }

    const fn new_non_origin(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Parses a generated road view id.
    #[must_use]
    pub fn from_view_id(view_id: &str) -> Option<Self> {
        let rest = view_id.strip_prefix(GRID_ROAD_VIEW_PREFIX)?;
        let (x, y) = rest.split_once("_y")?;
        let x = x.strip_prefix('x')?;
        Self::new(parse_signed_token(x)?, parse_signed_token(y)?)
    }

    /// Returns the generated road view id.
    #[must_use]
    pub fn view_id(self) -> String {
        format!(
            "{GRID_ROAD_VIEW_PREFIX}x{}_y{}",
            encode_signed(self.x),
            encode_signed(self.y)
        )
    }

    /// Horizontal grid coordinate.
    #[must_use]
    pub const fn x(self) -> i32 {
        self.x
    }

    /// Vertical grid coordinate.
    #[must_use]
    pub const fn y(self) -> i32 {
        self.y
    }

    /// Returns the neighboring road after a cardinal move.
    #[must_use]
    fn moved(self, direction: Direction) -> Option<GridRoadMoveTarget> {
        let (x, y) = match direction {
            Direction::North => (self.x, self.y.checked_add(1)?),
            Direction::South => (self.x, self.y.checked_sub(1)?),
            Direction::East => (self.x.checked_add(1)?, self.y),
            Direction::West => (self.x.checked_sub(1)?, self.y),
            Direction::Up | Direction::Down => return None,
        };
        Some(GridRoadMoveTarget::from_coordinates(x, y))
    }

    /// Player-facing road name.
    #[must_use]
    pub fn title(self) -> String {
        match (self.x, self.y) {
            (0, _) => horizontal_road_name(self.y),
            (_, 0) => vertical_road_name(self.x),
            _ => format!(
                "{} @ {}",
                vertical_road_name(self.x),
                horizontal_road_name(self.y)
            ),
        }
    }

    /// Compact coordinate code used inside building doorplates.
    #[must_use]
    pub fn code(self) -> String {
        format!(
            "{}-{}",
            axis_code(self.x, "E", "W"),
            axis_code(self.y, "N", "S")
        )
    }

    /// Building doorplates visible around this road crossing.
    #[must_use]
    pub fn parcel_addresses(self) -> Vec<GridParcelAddress> {
        (1..=4)
            .filter_map(|door| GridParcelAddress::new(self, door))
            .collect()
    }

    /// Builds a runtime view for this generated road.
    #[must_use]
    pub fn to_view(self) -> View {
        self.to_view_with_origin(GridOrigin::default_admission())
    }

    /// Builds a runtime view for this generated road using a static origin view.
    #[must_use]
    pub fn to_view_with_origin(self, origin: GridOrigin<'_>) -> View {
        let parcels = self.parcel_addresses();
        View {
            id: self.view_id(),
            title: self.title(),
            description: format!(
                "{} is a generated grid road. Roads have names only; the surrounding building cells carry doorplates. The grid has no fixed edge, so continuing with /go keeps extending the town.",
                self.title()
            ),
            ascii_art: self.ascii_art(&parcels),
            exits: self.exits(origin),
            entities: Vec::new(),
            layout: None,
        }
    }

    fn exits(self, origin: GridOrigin<'_>) -> Vec<Exit> {
        [
            Direction::North,
            Direction::South,
            Direction::East,
            Direction::West,
        ]
        .into_iter()
        .filter_map(|direction| {
            let target = self.moved(direction)?;
            let (target, label) = match target {
                GridRoadMoveTarget::Origin => {
                    (origin.view_id().to_owned(), origin.label().to_owned())
                }
                GridRoadMoveTarget::Road(road) => (road.view_id(), road.title()),
            };
            Some(Exit {
                direction,
                target,
                label: Some(label),
                requirements: Vec::new(),
            })
        })
        .collect()
    }

    fn ascii_art(self, parcels: &[GridParcelAddress]) -> Vec<String> {
        let door = |index: usize| {
            parcels
                .get(index)
                .map(|parcel| parcel.parcel_id())
                .unwrap_or_else(|| "-".to_owned())
        };
        vec![
            format!("        [{}]              [{}]", door(0), door(1)),
            "             |                    |".to_owned(),
            format!("-------------+ {} +-------------", self.title()),
            "             |        <Me>        |".to_owned(),
            format!("        [{}]              [{}]", door(2), door(3)),
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GridRoadMoveTarget {
    Origin,
    Road(GridRoad),
}

impl GridRoadMoveTarget {
    fn from_coordinates(x: i32, y: i32) -> Self {
        GridRoad::new(x, y).map_or(Self::Origin, Self::Road)
    }
}

/// Doorplate for a building cell adjacent to a generated road crossing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GridParcelAddress {
    road: GridRoad,
    door: u8,
}

impl GridParcelAddress {
    #[must_use]
    fn new(road: GridRoad, door: u8) -> Option<Self> {
        (1..=4).contains(&door).then_some(Self { road, door })
    }

    /// Parses a generated parcel id such as `E1-C0-01`.
    #[must_use]
    pub fn from_parcel_id(parcel_id: &str) -> Option<Self> {
        let mut parts = parcel_id.split('-');
        let x = parts.next()?;
        let y = parts.next()?;
        let door = parts.next()?;
        if parts.next().is_some() {
            return None;
        }
        let x = parse_axis_code(x, "E", "W")?;
        let y = parse_axis_code(y, "N", "S")?;
        let door = door.parse::<u8>().ok()?;
        Self::new(GridRoad::new(x, y)?, door)
    }

    /// Returns the canonical player-facing parcel id when the input is a grid doorplate.
    #[must_use]
    pub fn canonical_parcel_id(parcel_id: &str) -> Option<String> {
        Self::from_parcel_id(parcel_id).map(Self::parcel_id)
    }

    /// Parses a generated parcel room view id.
    #[must_use]
    pub fn from_view_id(view_id: &str) -> Option<Self> {
        let parcel_id = view_id.strip_prefix(GRID_PARCEL_VIEW_PREFIX)?;
        Self::from_parcel_id(parcel_id)
    }

    /// Road this building cell faces.
    #[must_use]
    pub const fn road(self) -> GridRoad {
        self.road
    }

    /// Door number on the adjacent road crossing.
    #[must_use]
    pub const fn door(self) -> u8 {
        self.door
    }

    /// Player-facing parcel id.
    #[must_use]
    pub fn parcel_id(self) -> String {
        format!("{}-{:02}", self.road.code(), self.door)
    }

    /// Runtime room view id for this building cell.
    #[must_use]
    pub fn view_id(self) -> String {
        format!("{GRID_PARCEL_VIEW_PREFIX}{}", self.parcel_id())
    }

    /// Front road view id where this doorplate is visible.
    #[must_use]
    pub fn front_view_id(self) -> String {
        self.road.view_id()
    }

    /// Storage district code for uniqueness and ordering.
    #[must_use]
    pub fn district(self) -> String {
        format!("grid:{}", self.road.code())
    }

    /// Storage position for uniqueness and ordering within the district.
    #[must_use]
    pub fn position(self) -> i32 {
        i32::from(self.door)
    }

    /// Builds a runtime view for this generated building cell.
    #[must_use]
    pub fn to_view(self) -> View {
        self.to_view_with_origin(GridOrigin::default_admission())
    }

    /// Builds a runtime view for this generated building cell.
    ///
    /// The origin parameter is accepted for symmetry with generated road construction; building
    /// cells always exit to their front road.
    #[must_use]
    pub fn to_view_with_origin(self, _origin: GridOrigin<'_>) -> View {
        let exit_direction = if self.door <= 2 {
            Direction::South
        } else {
            Direction::North
        };
        let exit_target = self.road.view_id();
        let exit_label = self.road.title();
        View {
            id: self.view_id(),
            title: format!("Doorplate {}", self.parcel_id()),
            description: format!(
                "Building cell {} faces {}. The building has the doorplate; the road keeps only its road name.",
                self.parcel_id(),
                self.road.title()
            ),
            ascii_art: vec![
                format!("                 [{}]", self.parcel_id()),
                "                    |".to_owned(),
                "                   <Me>".to_owned(),
                "                    |".to_owned(),
                format!(
                    "             {} to {}",
                    exit_direction.as_str(),
                    self.road.title()
                ),
            ],
            exits: vec![Exit {
                direction: exit_direction,
                target: exit_target,
                label: Some(exit_label),
                requirements: Vec::new(),
            }],
            entities: Vec::new(),
            layout: None,
        }
    }
}

/// Returns true when a view id belongs to the generated grid.
#[must_use]
pub fn is_grid_view_id(view_id: &str) -> bool {
    GridRoad::from_view_id(view_id).is_some() || GridParcelAddress::from_view_id(view_id).is_some()
}

/// Builds a generated grid view when the id is in the grid namespace.
#[must_use]
pub fn grid_view(view_id: &str) -> Option<View> {
    grid_view_with_origin(view_id, GridOrigin::default_admission())
}

/// Builds a generated grid view using a static origin view id when the id is in the grid namespace.
#[must_use]
pub fn grid_view_with_origin(view_id: &str, origin: GridOrigin<'_>) -> Option<View> {
    if let Some(road) = GridRoad::from_view_id(view_id) {
        return Some(road.to_view_with_origin(origin));
    }
    GridParcelAddress::from_view_id(view_id).map(|address| address.to_view_with_origin(origin))
}

/// Builds generated map ASCII for the grid origin or a generated grid view.
#[must_use]
pub fn generated_map_ascii_with_origin(
    view_id: &str,
    origin: GridOrigin<'_>,
) -> Option<Vec<String>> {
    if view_id == origin.view_id() {
        return Some(origin_ascii_art(origin));
    }
    grid_view_with_origin(view_id, origin).map(|view| view.ascii_art)
}

/// Builds the generated origin view from a static anchor view.
#[must_use]
pub fn generated_origin_view(source: &View, origin: GridOrigin<'_>) -> Option<View> {
    if source.id != origin.view_id() {
        return None;
    }
    Some(View {
        id: source.id.clone(),
        title: origin.label().to_owned(),
        description: source.description.clone(),
        ascii_art: origin_ascii_art(origin),
        exits: origin_exits(),
        entities: source.entities.clone(),
        layout: None,
    })
}

/// Player-facing generated-grid label for a view id.
#[must_use]
pub fn generated_grid_label(view_id: &str) -> Option<String> {
    if let Some(road) = GridRoad::from_view_id(view_id) {
        return Some(road.title());
    }
    GridParcelAddress::from_view_id(view_id)
        .map(|address| format!("Doorplate {}", address.parcel_id()))
}

fn origin_ascii_art(origin: GridOrigin<'_>) -> Vec<String> {
    let label = short_label(origin.label(), 18);
    vec![
        "             North 1 Rd.".to_owned(),
        "                  |".to_owned(),
        "      [C0-N1-01]  |  [C0-N1-02]".to_owned(),
        "        +----+----+----+----+".to_owned(),
        format!("West 1 Rd. + {label:^18} + East 1 Rd."),
        "        +----+----+----+----+".to_owned(),
        "      [C0-S1-03]  |  [C0-S1-04]".to_owned(),
        "                  |".to_owned(),
        "             South 1 Rd.".to_owned(),
    ]
}

fn origin_exits() -> Vec<Exit> {
    [
        (Direction::North, GridRoad::new_non_origin(0, 1)),
        (Direction::South, GridRoad::new_non_origin(0, -1)),
        (Direction::West, GridRoad::new_non_origin(-1, 0)),
        (Direction::East, GridRoad::new_non_origin(1, 0)),
    ]
    .into_iter()
    .map(|(direction, road)| Exit {
        direction,
        target: road.view_id(),
        label: Some(road.title()),
        requirements: Vec::new(),
    })
    .collect()
}

fn parse_signed_token(token: &str) -> Option<i32> {
    if token == "0" {
        return Some(0);
    }
    if let Some(rest) = token.strip_prefix('p') {
        let number = rest.parse::<i64>().ok()?;
        if number <= 0 || number > i64::from(i32::MAX) {
            return None;
        }
        return i32::try_from(number).ok();
    }
    let number = token.strip_prefix('m')?.parse::<i64>().ok()?;
    if number <= 0 || number > i64::from(i32::MAX) + 1 {
        return None;
    }
    i32::try_from(-number).ok()
}

fn encode_signed(value: i32) -> String {
    if value == 0 {
        "0".to_owned()
    } else if value > 0 {
        format!("p{value}")
    } else {
        format!("m{}", -(i64::from(value)))
    }
}

fn parse_axis_code(token: &str, positive: &str, negative: &str) -> Option<i32> {
    let token = token.to_ascii_uppercase();
    if token == "C0" {
        return Some(0);
    }
    if let Some(rest) = token.strip_prefix(positive) {
        let number = rest.parse::<i64>().ok()?;
        if number <= 0 || number > i64::from(i32::MAX) {
            return None;
        }
        return i32::try_from(number).ok();
    }
    if let Some(rest) = token.strip_prefix(negative) {
        let number = rest.parse::<i64>().ok()?;
        if number > 0 && number <= i64::from(i32::MAX) + 1 {
            return i32::try_from(-number).ok();
        }
    }
    None
}

fn axis_code(value: i32, positive: &str, negative: &str) -> String {
    if value == 0 {
        "C0".to_owned()
    } else if value > 0 {
        format!("{positive}{value}")
    } else {
        format!("{negative}{}", -(i64::from(value)))
    }
}

fn vertical_road_name(x: i32) -> String {
    if x == 0 {
        "Center Rd.".to_owned()
    } else if x > 0 {
        format!("East {x} Rd.")
    } else {
        format!("West {} Rd.", -(i64::from(x)))
    }
}

fn horizontal_road_name(y: i32) -> String {
    if y == 0 {
        "Harbor Cross Rd.".to_owned()
    } else if y > 0 {
        format!("North {y} Rd.")
    } else {
        format!("South {} Rd.", -(i64::from(y)))
    }
}

fn short_label(label: &str, max_chars: usize) -> String {
    let mut output = label.chars().take(max_chars).collect::<String>();
    if label.chars().count() > max_chars && max_chars > 1 {
        output.pop();
        output.push('~');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn road(x: i32, y: i32) -> GridRoad {
        GridRoad::new(x, y).expect("valid generated road")
    }

    #[test]
    fn road_view_id_round_trips_signed_coordinates() {
        let road = road(-12, 34);

        assert_eq!(GridRoad::from_view_id(&road.view_id()), Some(road));
    }

    #[test]
    fn road_title_uses_road_names_not_doorplates() {
        assert_eq!(road(1, 0).title(), "East 1 Rd.");
        assert_eq!(road(0, -2).title(), "South 2 Rd.");
        assert_eq!(road(2, 3).title(), "East 2 Rd. @ North 3 Rd.");
    }

    #[test]
    fn generated_road_exposes_doorplates_around_the_road() {
        let view = road(1, 0).to_view();

        assert_eq!(view.title, "East 1 Rd.");
        assert!(view.ascii_art.join("\n").contains("[E1-C0-01]"));
        assert!(view.ascii_art.join("\n").contains("East 1 Rd."));
    }

    #[test]
    fn generated_roads_return_to_static_harbor_square_at_origin() {
        let view = road(1, 0).to_view();
        let west = view
            .exits
            .iter()
            .find(|exit| exit.direction == Direction::West)
            .expect("west exit");

        assert_eq!(west.target, "arrival_street");
        assert_eq!(west.label.as_deref(), Some("Harbor Square"));
    }

    #[test]
    fn generated_roads_use_configured_origin_anchor() {
        let view =
            road(1, 0).to_view_with_origin(GridOrigin::new("custom_arrival", "Custom Arrival"));
        let west = view
            .exits
            .iter()
            .find(|exit| exit.direction == Direction::West)
            .expect("west exit");

        assert_eq!(west.target, "custom_arrival");
        assert_eq!(west.label.as_deref(), Some("Custom Arrival"));
    }

    #[test]
    fn generated_grid_rejects_plaza_origin_as_generated_road() {
        assert_eq!(GridRoad::new(0, 0), None);
        assert_eq!(GridRoad::from_view_id("grid_road_x0_y0"), None);
        assert_eq!(grid_view("grid_road_x0_y0"), None);
        assert_eq!(
            generated_map_ascii_with_origin("grid_road_x0_y0", GridOrigin::default_admission()),
            None
        );
        assert_eq!(generated_grid_label("grid_road_x0_y0"), None);
    }

    #[test]
    fn generated_grid_rejects_plaza_origin_parcels() {
        assert_eq!(GridParcelAddress::from_parcel_id("C0-C0-01"), None);
        assert_eq!(GridParcelAddress::canonical_parcel_id("c0-c0-1"), None);
        assert_eq!(GridParcelAddress::from_view_id("parcel_C0-C0-01"), None);
        assert_eq!(grid_view("parcel_C0-C0-01"), None);
        assert_eq!(
            generated_map_ascii_with_origin("parcel_C0-C0-01", GridOrigin::default_admission()),
            None
        );
        assert_eq!(generated_grid_label("parcel_C0-C0-01"), None);
    }

    #[test]
    fn generated_origin_map_draws_plaza_from_anchor_metadata() {
        let ascii = generated_map_ascii_with_origin(
            "custom_arrival",
            GridOrigin::new("custom_arrival", "Custom Arrival"),
        )
        .expect("origin map");
        let rendered = ascii.join("\n");

        assert!(rendered.contains("+----+----+----+----+"));
        assert!(rendered.contains("Custom Arrival"));
        assert!(rendered.contains("North 1 Rd."));
        assert!(rendered.contains("[C0-N1-01]"));
    }

    #[test]
    fn generated_origin_view_replaces_static_map_and_exits() {
        let source = View {
            id: "custom_arrival".to_owned(),
            title: "Custom Arrival".to_owned(),
            description: "A static anchor.".to_owned(),
            ascii_art: vec!["STALE MAP".to_owned()],
            exits: vec![Exit {
                direction: Direction::West,
                target: "legacy_wilderness".to_owned(),
                label: Some("wilderness".to_owned()),
                requirements: Vec::new(),
            }],
            entities: vec!["board".to_owned()],
            layout: None,
        };
        let view =
            generated_origin_view(&source, GridOrigin::new("custom_arrival", "Custom Arrival"))
                .expect("origin view");
        let labels = view
            .exits
            .iter()
            .filter_map(|exit| exit.label.as_deref())
            .collect::<Vec<_>>();

        assert!(!view.ascii_art.join("\n").contains("STALE MAP"));
        assert_eq!(view.entities, vec!["board".to_owned()]);
        assert_eq!(
            labels,
            vec!["North 1 Rd.", "South 1 Rd.", "West 1 Rd.", "East 1 Rd."]
        );
        assert!(
            view.exits
                .iter()
                .any(|exit| exit.target == "grid_road_x0_yp1")
        );
        assert!(
            view.exits
                .iter()
                .all(|exit| !exit.target.contains("legacy_wilderness"))
        );
    }

    #[test]
    fn generated_grid_label_is_derived_from_core_model() {
        assert_eq!(
            generated_grid_label("grid_road_xm2_yp3").as_deref(),
            Some("West 2 Rd. @ North 3 Rd.")
        );
        assert_eq!(
            generated_grid_label("parcel_W2-N3-04").as_deref(),
            Some("Doorplate W2-N3-04")
        );
        assert_eq!(generated_grid_label("west_main_street"), None);
    }

    #[test]
    fn generated_map_ascii_covers_roads_and_doorplates() {
        let road =
            generated_map_ascii_with_origin("grid_road_xm2_yp3", GridOrigin::default_admission())
                .expect("road map")
                .join("\n");
        let parcel =
            generated_map_ascii_with_origin("parcel_W2-N3-04", GridOrigin::default_admission())
                .expect("parcel map")
                .join("\n");

        assert!(road.contains("West 2 Rd. @ North 3 Rd."));
        assert!(road.contains("[W2-N3-04]"));
        assert!(parcel.contains("[W2-N3-04]"));
        assert!(parcel.contains("north to West 2 Rd. @ North 3 Rd."));
    }

    #[test]
    fn parcel_id_round_trips_to_front_road() {
        let address = GridParcelAddress::from_parcel_id("E2-S3-04").expect("valid address");

        assert_eq!(address.road(), road(2, -3));
        assert_eq!(address.front_view_id(), "grid_road_xp2_ym3");
        assert_eq!(
            GridParcelAddress::from_view_id(&address.view_id()),
            Some(address)
        );
    }

    #[test]
    fn parcel_id_canonicalizes_human_input() {
        assert_eq!(
            GridParcelAddress::canonical_parcel_id("e2-s3-4").as_deref(),
            Some("E2-S3-04")
        );
    }
}
