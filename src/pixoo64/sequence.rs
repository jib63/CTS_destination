// SPDX-License-Identifier: MIT
//! Editable device playlist. No changes to the shared web departure data.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Screen {
    Station,
    Miniature,
    Birthdays,
    Events,
}

impl Screen {
    pub fn departures(self) -> bool {
        matches!(self, Self::Station | Self::Miniature)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub screen: Screen,
    /// None expands to all configured platforms, in their configured order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    pub seconds: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sequence {
    pub steps: Vec<Step>,
    pub lines_per_screen: u8,
}

impl Sequence {
    /// Resolve saved explicit platforms against the current Arrêt selection.
    /// A completely replaced selection reuses the first step of each design
    /// for all new platforms, rather than leaving a moments-only playlist.
    pub fn for_platforms(&self, refs: &[String]) -> Self {
        let has_departures = self
            .steps
            .iter()
            .any(|s| s.screen.departures() && s.platform.as_ref().is_none_or(|p| refs.contains(p)));
        let mut seen = Vec::new();
        let steps = self
            .steps
            .iter()
            .filter_map(|s| {
                if !s.screen.departures() || s.platform.as_ref().is_none_or(|p| refs.contains(p)) {
                    return Some(s.clone());
                }
                if !has_departures && !seen.contains(&s.screen) {
                    seen.push(s.screen);
                    return Some(Step {
                        platform: None,
                        ..s.clone()
                    });
                }
                None
            })
            .collect();
        Self {
            steps,
            lines_per_screen: self.lines_per_screen,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.steps.is_empty() || self.steps.len() > 64 {
            return Err("Le cycle doit contenir entre 1 et 64 étapes.".into());
        }
        if !(1..=4).contains(&self.lines_per_screen) {
            return Err("Station doit afficher entre 1 et 4 lignes.".into());
        }
        if !self.steps.iter().any(|s| s.screen.departures()) {
            return Err("Ajoutez au moins un écran Station ou Strasbourg miniature.".into());
        }
        for (i, step) in self.steps.iter().enumerate() {
            if !(1..=300).contains(&step.seconds) {
                return Err(format!(
                    "Étape {} : choisissez une durée de 1 à 300 secondes.",
                    i + 1
                ));
            }
            if let Some(platform) = &step.platform {
                if !step.screen.departures()
                    || platform.trim().is_empty()
                    || platform.len() > 50
                    || platform.chars().any(char::is_control)
                {
                    return Err(format!("Étape {} : quai invalide.", i + 1));
                }
            }
        }
        Ok(())
    }

    /// Preserves the validated v1.6.1 cycle on first opening and for presets.
    pub fn preset(
        refs: &[String],
        tram: u32,
        moment: u32,
        lines: u8,
        screens: &[Screen],
        interleave: bool,
    ) -> Self {
        let platforms: Vec<_> = if refs.is_empty() {
            vec![None]
        } else {
            refs.iter().cloned().map(Some).collect()
        };
        let mut steps = Vec::new();
        let duration = |p: &Option<String>| {
            if p.as_deref() == Some("298B") {
                tram.saturating_mul(2)
            } else {
                tram
            }
        };
        let moments = |steps: &mut Vec<Step>| {
            for screen in [Screen::Birthdays, Screen::Events] {
                steps.push(Step {
                    screen,
                    platform: None,
                    seconds: moment,
                });
            }
        };
        if interleave {
            for platform in &platforms {
                for &screen in screens {
                    steps.push(Step {
                        screen,
                        platform: platform.clone(),
                        seconds: duration(platform),
                    });
                }
            }
            moments(&mut steps);
        } else {
            for &screen in screens {
                for platform in &platforms {
                    steps.push(Step {
                        screen,
                        platform: platform.clone(),
                        seconds: duration(platform),
                    });
                }
                moments(&mut steps);
            }
        }
        Self {
            steps,
            lines_per_screen: lines,
        }
    }
}

/// Replace just the TOML value, including manually formatted multiline values.
/// Spans let us preserve comments, secrets, unrelated settings and file permissions.
pub fn save(path: &str, sequence: &Sequence) -> anyhow::Result<()> {
    sequence.validate().map_err(anyhow::Error::msg)?;
    let content = std::fs::read_to_string(path)?;
    let root: std::collections::HashMap<String, toml::Spanned<toml::Value>> =
        toml::from_str(&content)?;
    let steps = sequence
        .steps
        .iter()
        .map(|s| {
            let name = serde_json::to_string(&s.screen).expect("screen enum");
            let platform = s
                .platform
                .as_ref()
                .map(|p| format!(", platform = {}", toml::Value::String(p.clone())))
                .unwrap_or_default();
            format!("{{ screen = {name}{platform}, seconds = {} }}", s.seconds)
        })
        .collect::<Vec<_>>()
        .join(", ");
    let value = format!(
        "{{ lines_per_screen = {}, steps = [{steps}] }}",
        sequence.lines_per_screen
    );
    let mut updated = content.clone();
    if let Some(old) = root.get("pixoo64_sequence") {
        updated.replace_range(old.span(), &value);
    } else {
        updated = format!("pixoo64_sequence = {value}\n{content}");
    }
    let parsed: toml::Table = toml::from_str(&updated)?;
    let roundtrip: Sequence = parsed["pixoo64_sequence"].clone().try_into()?;
    anyhow::ensure!(roundtrip == *sequence, "Invalid saved sequence");
    // Write the existing file in place: hardened instances grant access to this
    // file only, and its owner/group/mode must not change.
    std::fs::write(path, updated)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_removal_and_replacement_keep_a_valid_cycle() {
        let seq = Sequence::preset(
            &["298B".into(), "298A".into()],
            6,
            1,
            4,
            &[Screen::Station, Screen::Miniature],
            false,
        );
        let single = seq.for_platforms(&["298B".into()]);
        assert_eq!(
            single.steps.iter().map(|s| s.seconds).collect::<Vec<_>>(),
            vec![12, 1, 1, 12, 1, 1]
        );
        assert!(
            single
                .steps
                .iter()
                .filter_map(|s| s.platform.as_deref())
                .all(|p| p == "298B")
        );
        let replaced = seq.for_platforms(&["new".into()]);
        assert_eq!(replaced.steps.len(), 6);
        assert!(replaced.steps.iter().all(|s| s.platform.is_none()));
        assert!(single.validate().is_ok() && replaced.validate().is_ok());
        assert_eq!(seq.for_platforms(&["298B".into(), "298A".into()]), seq);
    }
    #[test]
    fn defaults_preserve_validated_cycle_and_reject_invalid_settings() {
        let mut seq = Sequence::preset(
            &["298B".into(), "298A".into()],
            6,
            1,
            4,
            &[Screen::Station, Screen::Miniature],
            false,
        );
        assert_eq!(
            seq.steps.iter().map(|s| s.seconds).collect::<Vec<_>>(),
            vec![12, 6, 1, 1, 12, 6, 1, 1]
        );
        assert!(seq.validate().is_ok());
        seq.steps[0].seconds = 0;
        assert!(seq.validate().is_err());
        seq.steps[0].seconds = 301;
        assert!(seq.validate().is_err());
        seq.steps.clear();
        assert!(seq.validate().is_err());
        assert!(serde_json::from_str::<Step>(r#"{"screen":"bad","seconds":1}"#).is_err());
    }
    #[test]
    fn persistence_preserves_other_fields_comments_and_multiline_values() {
        let path = std::env::temp_dir().join(format!(
            "cts-sequence-{}-{}.toml",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let original = "# Keep this comment\ncts_api_token = \"fictional-test-token\"\ncts_monitoring_ref = [\"298B\", \"298A\"]\n[extra]\nlabel = \"untouched\"\n";
        std::fs::write(&path, original).unwrap();
        let seq = Sequence::preset(&["298B".into()], 6, 1, 4, &[Screen::Miniature], false);
        save(path.to_str().unwrap(), &seq).unwrap();
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.ends_with(original));
        let multiline = format!(
            "pixoo64_sequence = {{\n lines_per_screen = 2,\n steps = [{{screen = \"station\", seconds = 8}}]\n}}\n{original}"
        );
        std::fs::write(&path, multiline).unwrap();
        save(path.to_str().unwrap(), &seq).unwrap();
        assert_eq!(saved, std::fs::read_to_string(&path).unwrap());
        std::fs::remove_file(path).unwrap();
    }
}
