// SPDX-License-Identifier: MIT
//! Display-only stop metadata, independent of the availability of departures.
use crate::{cts::model::MonitoredStopVisit, web::AppState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    pub name: String,
    #[serde(default)]
    pub directions: Vec<String>,
}
pub type Labels = BTreeMap<String, Label>;

/// Initial labels for the Jean Jaurès platforms supported by the default cycle.
/// Observed CTS metadata takes precedence; references remain internal identifiers.
pub fn initial(reference: &str) -> Label {
    let directions = match reference {
        "298B" => vec!["C → Gare Centrale", "D → Poteries"],
        "298A" => vec!["C → Neuhof Rodolphe Reuss", "D → Kehl / Port du Rhin"],
        _ => return Label::default(),
    };
    Label {
        name: "Jean Jaurès".into(),
        directions: directions.into_iter().map(String::from).collect(),
    }
}

pub fn observations(visits: &[MonitoredStopVisit]) -> Labels {
    let mut result = Labels::new();
    for visit in visits {
        let j = &visit.monitored_vehicle_journey;
        let label = result.entry(visit.stop_code.clone()).or_default();
        label.name = j.monitored_call.stop_point_name.clone();
        if !j.published_line_name.trim().is_empty() && !j.destination_name.trim().is_empty() {
            let mut direction = format!("{} → {}", j.published_line_name, j.destination_name);
            if let Some(via) = j.via.as_ref().filter(|v| !v.is_empty()) {
                direction.push_str(&format!(" via {via}"));
            }
            label.directions.push(direction);
        }
    }
    result
}

pub fn merge(labels: &mut Labels, observations: Labels) {
    for (reference, observed) in observations {
        if observed.name.trim().is_empty() && observed.directions.is_empty() {
            continue;
        }
        let label = labels.entry(reference).or_default();
        if !observed.name.trim().is_empty() {
            label.name = observed.name;
        }
        label.directions.extend(
            observed
                .directions
                .into_iter()
                .filter(|d| !d.trim().is_empty()),
        );
        label.directions.sort();
        label.directions.dedup();
    }
}

/// Save only public labels in the already-writable configuration file. No extra
/// directory permissions or background CTS queries are required.
fn save(path: &str, labels: &Labels) -> anyhow::Result<()> {
    let content = std::fs::read_to_string(path)?;
    let root: BTreeMap<String, toml::Spanned<toml::Value>> = toml::from_str(&content)?;
    let quote = |s: &str| toml::Value::String(s.into()).to_string();
    let entries = labels
        .iter()
        .map(|(reference, label)| {
            let directions = label
                .directions
                .iter()
                .map(|d| quote(d))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "{} = {{ name = {}, directions = [{}] }}",
                quote(reference),
                quote(&label.name),
                directions
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let value = format!("{{ {entries} }}");
    let mut updated = content.clone();
    if let Some(old) = root.get("pixoo64_platform_labels") {
        updated.replace_range(old.span(), &value);
    } else {
        updated = format!("pixoo64_platform_labels = {value}\n{content}");
    }
    let parsed: toml::Table = toml::from_str(&updated)?;
    let roundtrip: Labels = parsed["pixoo64_platform_labels"].clone().try_into()?;
    anyhow::ensure!(&roundtrip == labels, "Invalid platform labels");
    std::fs::write(path, updated)?;
    Ok(())
}

pub async fn remember(state: &AppState, observations: Labels) {
    if !state.pixoo64_enabled || state.cts_simulation || observations.is_empty() {
        return;
    }
    let _edit = state.config_edit_lock.lock().await;
    let mut labels = state.pixoo64_platform_labels.write().await;
    let before = labels.clone();
    merge(&mut labels, observations);
    if *labels != before {
        if let Err(error) = save(&state.config_path, &labels) {
            tracing::warn!(%error, "Unable to persist PIXOO64 platform labels; retained in memory");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cts_visits_identify_each_physical_platform_without_mixing_directions() {
        let visit = |reference: &str, destination: &str| {
            serde_json::from_value::<MonitoredStopVisit>(serde_json::json!({
                "StopCode": reference,
                "MonitoredVehicleJourney": {
                    "PublishedLineName": "C", "DestinationName": destination,
                    "DestinationShortName": destination, "VehicleMode": "tram", "Via": null,
                    "MonitoredCall": {"StopPointName": "Jean Jaurès",
                        "ExpectedArrivalTime": "2026-10-24T08:30:00+02:00",
                        "Extension": {"IsRealTime": true}}
                }
            }))
            .unwrap()
        };
        let labels = observations(&[visit("298B", "Gare Centrale"), visit("298A", "Neuhof")]);
        assert_eq!(labels["298B"].directions, vec!["C → Gare Centrale"]);
        assert_eq!(labels["298A"].directions, vec!["C → Neuhof"]);
        assert_eq!(labels["298A"].name, "Jean Jaurès");
    }
    #[test]
    fn labels_survive_empty_partial_departures_and_restart() {
        let mut labels = Labels::from([(
            "other-quai".into(),
            Label {
                name: "Autre arrêt".into(),
                directions: vec!["A → Nord".into()],
            },
        )]);
        merge(
            &mut labels,
            Labels::from([("other-quai".into(), Label::default())]),
        );
        merge(
            &mut labels,
            Labels::from([(
                "other-quai".into(),
                Label {
                    name: "Autre arrêt".into(),
                    directions: vec!["A → Sud".into(), "A → Nord".into()],
                },
            )]),
        );
        assert_eq!(labels["other-quai"].directions, vec!["A → Nord", "A → Sud"]);
        let path = std::env::temp_dir().join(format!(
            "cts-labels-{}-{}.toml",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let original =
            "# unchanged\ncts_api_token = 'fictional'\ncts_monitoring_ref = ['other-quai']\n";
        std::fs::write(&path, original).unwrap();
        save(path.to_str().unwrap(), &labels).unwrap();
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.ends_with(original));
        save(path.to_str().unwrap(), &labels).unwrap();
        assert_eq!(saved, std::fs::read_to_string(&path).unwrap());
        let parsed: toml::Table = toml::from_str(&saved).unwrap();
        let loaded: Labels = parsed["pixoo64_platform_labels"]
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(labels, loaded);
        assert_eq!(initial("298B").name, "Jean Jaurès");
        assert_ne!(initial("298A").directions, initial("298B").directions);
        assert!(initial("unknown").name.is_empty());
        std::fs::remove_file(path).unwrap();
    }
}
