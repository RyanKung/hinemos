//! Infinite grid-road and building-address model.

use crate::command::Direction;
use crate::model::{DEFAULT_ADMISSION_VIEW_ID, Exit, View};

/// Prefix for generated road view ids.
pub const GRID_ROAD_VIEW_PREFIX: &str = "grid_road_";
/// Prefix for generated building view ids.
pub const GRID_PARCEL_VIEW_PREFIX: &str = "parcel_";

/// Coordinate of a generated road crossing in the infinite town grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GridRoad {
    x: i32,
    y: i32,
}

impl GridRoad {
    /// Creates a grid-road coordinate.
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Parses a generated road view id.
    #[must_use]
    pub fn from_view_id(view_id: &str) -> Option<Self> {
        let rest = view_id.strip_prefix(GRID_ROAD_VIEW_PREFIX)?;
        let (x, y) = rest.split_once("_y")?;
        let x = x.strip_prefix('x')?;
        Some(Self {
            x: parse_signed_token(x)?,
            y: parse_signed_token(y)?,
        })
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
    pub fn moved(self, direction: Direction) -> Option<Self> {
        match direction {
            Direction::North => Some(Self::new(self.x, self.y.checked_add(1)?)),
            Direction::South => Some(Self::new(self.x, self.y.checked_sub(1)?)),
            Direction::East => Some(Self::new(self.x.checked_add(1)?, self.y)),
            Direction::West => Some(Self::new(self.x.checked_sub(1)?, self.y)),
            Direction::Up | Direction::Down => None,
        }
    }

    /// Player-facing road name.
    #[must_use]
    pub fn title(self) -> String {
        match (self.x, self.y) {
            (0, 0) => "Harbor Square Grid".to_owned(),
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
        let parcels = self.parcel_addresses();
        View {
            id: self.view_id(),
            title: self.title(),
            description: format!(
                "{} is a generated grid road. Roads have names only; the surrounding building cells carry doorplates. The grid has no fixed edge, so continuing with /go keeps extending the town.",
                self.title()
            ),
            ascii_art: self.ascii_art(&parcels),
            exits: self.exits(),
            entities: Vec::new(),
            layout: None,
        }
    }

    fn exits(self) -> Vec<Exit> {
        [
            Direction::North,
            Direction::South,
            Direction::East,
            Direction::West,
        ]
        .into_iter()
        .filter_map(|direction| {
            let target = self.moved(direction)?;
            let (target, label) = if target == Self::new(0, 0) {
                (
                    DEFAULT_ADMISSION_VIEW_ID.to_owned(),
                    "Harbor Square".to_owned(),
                )
            } else {
                (target.view_id(), target.title())
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
            "------------------------------------------------------------".to_owned(),
            format!("                    {}", self.title().to_ascii_uppercase()),
            "------------------------------------------------------------".to_owned(),
            format!("        [{}] | {} | [{}]", door(0), self.title(), door(1)),
            "                 |        <Me>        |".to_owned(),
            format!("        [{}] |                  | [{}]", door(2), door(3)),
        ]
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
        Self::new(GridRoad::new(x, y), door)
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
        let exit_direction = if self.door <= 2 {
            Direction::South
        } else {
            Direction::North
        };
        View {
            id: self.view_id(),
            title: format!("Doorplate {}", self.parcel_id()),
            description: format!(
                "Building cell {} faces {}. The building has the doorplate; the road keeps only its road name.",
                self.parcel_id(),
                self.road.title()
            ),
            ascii_art: vec![
                "------------------------------------------------------------".to_owned(),
                format!("                    DOORPLATE {}", self.parcel_id()),
                "------------------------------------------------------------".to_owned(),
                format!("                 [{}]", self.parcel_id()),
                "                   <Me>".to_owned(),
                format!(
                    "             {} to {}",
                    exit_direction.as_str(),
                    self.road.title()
                ),
            ],
            exits: vec![Exit {
                direction: exit_direction,
                target: self.road.view_id(),
                label: Some(self.road.title()),
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
    if let Some(road) = GridRoad::from_view_id(view_id) {
        return Some(road.to_view());
    }
    GridParcelAddress::from_view_id(view_id).map(GridParcelAddress::to_view)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn road_view_id_round_trips_signed_coordinates() {
        let road = GridRoad::new(-12, 34);

        assert_eq!(GridRoad::from_view_id(&road.view_id()), Some(road));
    }

    #[test]
    fn road_title_uses_road_names_not_doorplates() {
        assert_eq!(GridRoad::new(1, 0).title(), "East 1 Rd.");
        assert_eq!(GridRoad::new(0, -2).title(), "South 2 Rd.");
        assert_eq!(GridRoad::new(2, 3).title(), "East 2 Rd. @ North 3 Rd.");
    }

    #[test]
    fn generated_road_exposes_doorplates_around_the_road() {
        let view = GridRoad::new(1, 0).to_view();

        assert_eq!(view.title, "East 1 Rd.");
        assert!(view.ascii_art.join("\n").contains("[E1-C0-01]"));
        assert!(view.ascii_art.join("\n").contains("East 1 Rd."));
    }

    #[test]
    fn generated_roads_return_to_static_harbor_square_at_origin() {
        let view = GridRoad::new(1, 0).to_view();
        let west = view
            .exits
            .iter()
            .find(|exit| exit.direction == Direction::West)
            .expect("west exit");

        assert_eq!(west.target, "arrival_street");
        assert_eq!(west.label.as_deref(), Some("Harbor Square"));
    }

    #[test]
    fn parcel_id_round_trips_to_front_road() {
        let address = GridParcelAddress::from_parcel_id("E2-S3-04").expect("valid address");

        assert_eq!(address.road(), GridRoad::new(2, -3));
        assert_eq!(address.front_view_id(), "grid_road_xp2_ym3");
        assert_eq!(
            GridParcelAddress::from_view_id(&address.view_id()),
            Some(address)
        );
    }
}
