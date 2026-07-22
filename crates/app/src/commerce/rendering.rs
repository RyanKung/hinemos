use super::contracts::ParcelView;
use crate::{PARCEL_STATUS_BUILT, PARCEL_STATUS_CLAIMED};

pub(super) fn non_empty(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    if value.is_empty() { None } else { Some(value) }
}

pub(crate) fn render_parcel_list(parcels: &[impl ParcelView]) -> String {
    let mut lines = vec!["Parcels".to_owned()];
    let mut vacant_count = 0_u32;
    for parcel in parcels {
        match parcel.status() {
            PARCEL_STATUS_BUILT => lines.push(format!(
                "- {}: {}. Owner: {}. Enter from street: /enter {}.",
                parcel.parcel_id(),
                parcel.title().unwrap_or("built parcel"),
                parcel.owner_user().unwrap_or("unknown"),
                parcel.parcel_id()
            )),
            PARCEL_STATUS_CLAIMED => lines.push(format!(
                "- {}: claimed by {}; not built yet.",
                parcel.parcel_id(),
                parcel.owner_user().unwrap_or("unknown")
            )),
            _ => {
                vacant_count += 1;
                lines.push(format!(
                    "- {}: vacant. Claim: /parcel claim {}.",
                    parcel.parcel_id(),
                    parcel.parcel_id()
                ));
            }
        }
    }
    if vacant_count == 0 {
        lines
            .push("No vacant parcels right now. Use /parcel info <parcel> for details.".to_owned());
    } else {
        lines.push(format!(
            "{vacant_count} vacant parcel(s). Use /parcel claim <parcel>, /parcel token <parcel>, /parcel info <parcel>, or /parcel transfer <parcel> <user>."
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

pub(super) fn render_parcel_detail(parcel: &impl ParcelView) -> String {
    format!(
        "Parcel {}\nView: {}\nDistrict: {} {}\nStatus: {}\nOwner: {}\nRoom mail: {}\nTitle: {}\nDescription: {}\nStyle: {}\nPrompt: {}\nCommands: {}\n\n",
        parcel.parcel_id(),
        parcel.view_id(),
        parcel.district(),
        parcel.position(),
        parcel.status(),
        parcel.owner_user().unwrap_or("-"),
        parcel.room_user().unwrap_or("-"),
        parcel.title().unwrap_or("-"),
        parcel.description().unwrap_or("-"),
        parcel.style().unwrap_or("-"),
        parcel.operator_prompt().unwrap_or("-"),
        parcel.custom_commands().unwrap_or("-")
    )
}

pub(super) fn custom_command_preview(parcel: &impl ParcelView, raw_input: &str) -> Option<String> {
    let input_tokens = raw_input.split_whitespace().collect::<Vec<_>>();
    if input_tokens.is_empty() {
        return None;
    }
    let commands = parcel.custom_commands()?;
    let mut best = None::<(usize, String)>;
    for entry in commands.split(['\n', ';']) {
        let entry = entry.trim();
        let literal_tokens = command_literal_tokens(entry);
        if literal_tokens.is_empty() || !literal_prefix_matches(&input_tokens, &literal_tokens) {
            continue;
        }
        let Some(preview) = command_field_value(entry, "preview=") else {
            continue;
        };
        let preview = preview.trim();
        if !preview.is_empty() {
            let specificity = literal_tokens.len();
            if match best.as_ref() {
                Some((current, _)) => specificity > *current,
                None => true,
            } {
                best = Some((specificity, preview.to_owned()));
            }
        }
    }
    best.map(|(_, preview)| preview)
}

pub(super) fn is_custom_command_input(parcel: &impl ParcelView, raw_input: &str) -> bool {
    let Some(input_command) = raw_input.split_whitespace().next() else {
        return false;
    };
    custom_command_inputs(parcel)
        .any(|command| command.split_whitespace().next() == Some(input_command))
}

fn custom_command_inputs(parcel: &impl ParcelView) -> impl Iterator<Item = String> + '_ {
    parcel
        .custom_commands()
        .unwrap_or_default()
        .split(['\n', ';'])
        .map(str::trim)
        .filter(|command| command.starts_with('/'))
        .map(str::to_owned)
}

fn command_literal_tokens(entry: &str) -> Vec<&str> {
    entry
        .split_whitespace()
        .take_while(|token| {
            !token.contains('=')
                && !(token.starts_with('<') && token.ends_with('>'))
                && *token != "--"
        })
        .collect()
}

fn literal_prefix_matches(input_tokens: &[&str], literal_tokens: &[&str]) -> bool {
    input_tokens.len() >= literal_tokens.len()
        && input_tokens
            .iter()
            .zip(literal_tokens.iter())
            .all(|(input, literal)| input.eq_ignore_ascii_case(literal))
}

fn command_field_value(entry: &str, field: &str) -> Option<String> {
    let start = entry.find(field)? + field.len();
    let value = entry[start..].trim_start();
    if let Some(rest) = value.strip_prefix('"') {
        let end = rest.find('"').unwrap_or(rest.len());
        return Some(rest[..end].trim().to_owned());
    }
    if let Some(rest) = value.strip_prefix('\'') {
        let end = rest.find('\'').unwrap_or(rest.len());
        return Some(rest[..end].trim().to_owned());
    }
    Some(
        value
            .split_whitespace()
            .next()
            .unwrap_or(value)
            .trim()
            .to_owned(),
    )
}
