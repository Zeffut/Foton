//! What drives an objective's scores.
//!
//! Vanilla parity: `ObjectiveCriteria`. A criterion is a name; the fixed ones
//! below, the per-team-color kill criteria and every `<stat type>:<value>`
//! statistic are the names `ObjectiveCriteria.byName` accepts.

use foton_protocol::packets::game::{DisplaySlot, ObjectiveRenderType};
use foton_registry::registry::{REGISTRY, RegistryExt as _};
use foton_registry::stat::StatValueRegistry;
use foton_utils::Identifier;

/// `ObjectiveCriteria.TRIGGER`.
const TRIGGER: &str = "trigger";
/// The criteria `ObjectiveCriteria.registerCustom` registers one by one;
/// `(name, read only, default render type)`.
const CUSTOM: [(&str, bool, ObjectiveRenderType); 11] = [
    ("dummy", false, ObjectiveRenderType::Integer),
    (TRIGGER, false, ObjectiveRenderType::Integer),
    ("deathCount", false, ObjectiveRenderType::Integer),
    ("playerKillCount", false, ObjectiveRenderType::Integer),
    ("totalKillCount", false, ObjectiveRenderType::Integer),
    ("health", true, ObjectiveRenderType::Hearts),
    ("food", true, ObjectiveRenderType::Integer),
    ("air", true, ObjectiveRenderType::Integer),
    ("armor", true, ObjectiveRenderType::Integer),
    ("xp", true, ObjectiveRenderType::Integer),
    ("level", true, ObjectiveRenderType::Integer),
];

/// A resolved objective criterion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectiveCriteria {
    name: String,
    read_only: bool,
    render_type: ObjectiveRenderType,
}

impl ObjectiveCriteria {
    /// `ObjectiveCriteria.DUMMY`, which only commands change.
    #[must_use]
    pub fn dummy() -> Self {
        Self::custom("dummy").unwrap_or_else(|| Self {
            name: "dummy".to_owned(),
            read_only: false,
            render_type: ObjectiveRenderType::Integer,
        })
    }

    /// `ObjectiveCriteria.byName`: the criterion with this name, if there is one.
    #[must_use]
    pub fn by_name(name: &str) -> Option<Self> {
        Self::custom(name)
            .or_else(|| team_criteria(name))
            .or_else(|| stat_criteria(name))
    }

    fn custom(name: &str) -> Option<Self> {
        CUSTOM
            .iter()
            .find(|(custom, ..)| *custom == name)
            .map(|&(name, read_only, render_type)| Self {
                name: name.to_owned(),
                read_only,
                render_type,
            })
    }

    /// `ObjectiveCriteria.getName`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// `ObjectiveCriteria.isReadOnly`: whether commands may not write scores.
    #[must_use]
    pub const fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// `ObjectiveCriteria.getDefaultRenderType`.
    #[must_use]
    pub const fn default_render_type(&self) -> ObjectiveRenderType {
        self.render_type
    }

    /// Whether this is `ObjectiveCriteria.TRIGGER`.
    #[must_use]
    pub fn is_trigger(&self) -> bool {
        self.name == TRIGGER
    }

    /// Every name `/scoreboard objectives add` suggests: the fixed criteria,
    /// the team-color criteria and each statistic.
    ///
    /// Vanilla parity: `ObjectiveCriteriaArgument.listSuggestions`.
    #[must_use]
    pub fn suggestions() -> Vec<String> {
        let mut names: Vec<String> = CUSTOM.iter().map(|(name, ..)| (*name).to_owned()).collect();
        for color in team_color_names() {
            names.push(format!("teamkill.{color}"));
            names.push(format!("killedByTeam.{color}"));
        }
        let registry = &*REGISTRY;
        for (_, stat_type) in registry.stat_types.iter() {
            let values: Vec<Identifier> = match stat_type.value_registry {
                StatValueRegistry::Block => {
                    registry.blocks.iter().map(|(_, e)| e.key.clone()).collect()
                }
                StatValueRegistry::Item => {
                    registry.items.iter().map(|(_, e)| e.key.clone()).collect()
                }
                StatValueRegistry::EntityType => registry
                    .entity_types
                    .iter()
                    .map(|(_, e)| e.key.clone())
                    .collect(),
                StatValueRegistry::CustomStat => registry
                    .custom_stats
                    .iter()
                    .map(|(_, e)| e.key.clone())
                    .collect(),
            };
            names.extend(values.iter().map(|value| stat_name(&stat_type.key, value)));
        }
        names
    }
}

/// The team color spellings, which are the suffixes of the team display slots.
fn team_color_names() -> impl Iterator<Item = &'static str> {
    DisplaySlot::VALUES
        .into_iter()
        .filter_map(|slot| slot.serialized_name().strip_prefix("sidebar.team."))
}

/// `teamkill.<color>` and `killedByTeam.<color>`.
fn team_criteria(name: &str) -> Option<ObjectiveCriteria> {
    let color = name
        .strip_prefix("teamkill.")
        .or_else(|| name.strip_prefix("killedByTeam."))?;
    team_color_names()
        .any(|known| known == color)
        .then(|| ObjectiveCriteria {
            name: name.to_owned(),
            read_only: false,
            render_type: ObjectiveRenderType::Integer,
        })
}

/// `Stat.buildName`: both keys with `:` written as `.`, joined by a colon.
fn stat_name(stat_type: &Identifier, value: &Identifier) -> String {
    format!(
        "{}:{}",
        stat_type.to_string().replace(':', "."),
        value.to_string().replace(':', ".")
    )
}

/// `Identifier.bySeparator(string, '.')`: the first `.` separates the namespace.
fn identifier_by_dot(raw: &str) -> Option<Identifier> {
    let (namespace, path) = match raw.split_once('.') {
        Some((namespace, path)) if !path.is_empty() => (namespace, path),
        _ => ("minecraft", raw),
    };
    format!("{namespace}:{path}").parse().ok()
}

/// A statistic criterion: a stat type and one of its values.
///
/// Vanilla parity: the statistic branch of `ObjectiveCriteria.byName`.
fn stat_criteria(name: &str) -> Option<ObjectiveCriteria> {
    let (type_name, value_name) = name.split_once(':')?;
    let registry = &*REGISTRY;
    let stat_type = registry.stat_types.by_key(&identifier_by_dot(type_name)?)?;
    let value = identifier_by_dot(value_name)?;
    let known = match stat_type.value_registry {
        StatValueRegistry::Block => registry.blocks.by_key(&value).is_some(),
        StatValueRegistry::Item => registry.items.by_key(&value).is_some(),
        StatValueRegistry::EntityType => registry.entity_types.by_key(&value).is_some(),
        StatValueRegistry::CustomStat => registry.custom_stats.by_key(&value).is_some(),
    };
    known.then(|| ObjectiveCriteria {
        name: name.to_owned(),
        read_only: false,
        render_type: ObjectiveRenderType::Integer,
    })
}

#[cfg(test)]
mod tests {
    use foton_registry::init_vanilla_registry;

    use super::ObjectiveCriteria;
    use foton_protocol::packets::game::ObjectiveRenderType;

    #[test]
    fn vanilla_criteria_resolve_with_their_flags() {
        init_vanilla_registry();
        let health = ObjectiveCriteria::by_name("health").expect("health is a criterion");
        assert!(health.is_read_only());
        assert_eq!(health.default_render_type(), ObjectiveRenderType::Hearts);
        assert!(!ObjectiveCriteria::dummy().is_read_only());
        assert!(
            ObjectiveCriteria::by_name("trigger")
                .expect("trigger is a criterion")
                .is_trigger()
        );
        assert!(ObjectiveCriteria::by_name("teamkill.dark_red").is_some());
        assert!(ObjectiveCriteria::by_name("killedByTeam.reset").is_none());
        assert!(ObjectiveCriteria::by_name("Dummy").is_none());
    }

    /// Statistic criteria spell both keys with dots and are checked against
    /// the registries, so a typo in either half is refused.
    #[test]
    fn statistic_criteria_are_checked_against_the_registries() {
        init_vanilla_registry();
        assert!(ObjectiveCriteria::by_name("minecraft.custom:minecraft.jump").is_some());
        assert!(ObjectiveCriteria::by_name("minecraft.mined:minecraft.stone").is_some());
        assert!(ObjectiveCriteria::by_name("custom:jump").is_some());
        assert!(ObjectiveCriteria::by_name("minecraft.custom:minecraft.not_a_stat").is_none());
        assert!(ObjectiveCriteria::by_name("minecraft.nonsense:minecraft.jump").is_none());
        assert!(
            ObjectiveCriteria::suggestions()
                .iter()
                .any(|name| name == "minecraft.killed:minecraft.zombie")
        );
    }
}
