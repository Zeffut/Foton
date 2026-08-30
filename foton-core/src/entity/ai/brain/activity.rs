//! Vanilla `Activity`.

/// One named slice of a brain's behavior schedule.
///
/// Vanilla parity: `net.minecraft.world.entity.schedule.Activity`.
///
/// Vanilla registers these into `BuiltInRegistries.ACTIVITY`, but the registry
/// is a hardcoded Java list -- no datapack can add to it, no packet carries an
/// activity id and nothing writes one to disk. `FotonExtractor` emits no
/// `activity` asset, so mirroring the Java constants as an enum keeps the data
/// coming from the vanilla source without inventing a registry that would only
/// ever hold these twenty-six entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Activity {
    /// Behaviors that run whatever else the mob is doing.
    Core,
    /// The fallback activity every brain starts in.
    Idle,
    Work,
    Play,
    Rest,
    Meet,
    Panic,
    Raid,
    PreRaid,
    Hide,
    Fight,
    Celebrate,
    AdmireItem,
    Avoid,
    Ride,
    PlayDead,
    LongJump,
    Ram,
    Tongue,
    Swim,
    LaySpawn,
    Sniff,
    Investigate,
    Roar,
    Emerge,
    Dig,
}

impl Activity {
    /// Returns the registry path vanilla registers this activity under.
    ///
    /// Vanilla parity: `Activity.getName`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Idle => "idle",
            Self::Work => "work",
            Self::Play => "play",
            Self::Rest => "rest",
            Self::Meet => "meet",
            Self::Panic => "panic",
            Self::Raid => "raid",
            Self::PreRaid => "pre_raid",
            Self::Hide => "hide",
            Self::Fight => "fight",
            Self::Celebrate => "celebrate",
            Self::AdmireItem => "admire_item",
            Self::Avoid => "avoid",
            Self::Ride => "ride",
            Self::PlayDead => "play_dead",
            Self::LongJump => "long_jump",
            Self::Ram => "ram",
            Self::Tongue => "tongue",
            Self::Swim => "swim",
            Self::LaySpawn => "lay_spawn",
            Self::Sniff => "sniff",
            Self::Investigate => "investigate",
            Self::Roar => "roar",
            Self::Emerge => "emerge",
            Self::Dig => "dig",
        }
    }

    /// Every activity, in the order vanilla registers them.
    ///
    /// Vanilla parity: the `BuiltInRegistries.ACTIVITY` contents, which
    /// [`Self::by_key`] searches the way `Activity.CODEC` does.
    const ALL: &'static [Self] = &[
        Self::Core,
        Self::Idle,
        Self::Work,
        Self::Play,
        Self::Rest,
        Self::Meet,
        Self::Panic,
        Self::Raid,
        Self::PreRaid,
        Self::Hide,
        Self::Fight,
        Self::Celebrate,
        Self::AdmireItem,
        Self::Avoid,
        Self::Ride,
        Self::PlayDead,
        Self::LongJump,
        Self::Ram,
        Self::Tongue,
        Self::Swim,
        Self::LaySpawn,
        Self::Sniff,
        Self::Investigate,
        Self::Roar,
        Self::Emerge,
        Self::Dig,
    ];

    /// Looks an activity up by the key a timeline keyframe stores.
    ///
    /// Vanilla parity: `BuiltInRegistries.ACTIVITY.byNameCodec()`, which the
    /// `AttributeTypes.ACTIVITY` value codec is. A bare path resolves in the
    /// `minecraft` namespace, as every `ResourceLocation` does.
    #[must_use]
    pub fn by_key(key: &str) -> Option<Self> {
        let path = key.strip_prefix("minecraft:").unwrap_or(key);
        if path.contains(':') {
            return None;
        }
        Self::ALL.iter().copied().find(|entry| entry.name() == path)
    }
}

/// The environment attribute a brain reads its scheduled activity from.
///
/// Vanilla parity: the `EnvironmentAttribute<Activity>` held in `Brain.schedule`
/// and passed to `Brain.setSchedule`. 26.2 replaced the old `Schedule` /
/// `ScheduleBuilder` data type with a timeline track, so a "schedule" is now
/// the name of an activity-valued environment attribute; vanilla declares
/// exactly the two below in `EnvironmentAttributes` and nothing can add a
/// third, so this is a closed set rather than a registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleAttribute {
    /// Vanilla `EnvironmentAttributes.VILLAGER_ACTIVITY`.
    VillagerActivity,
    /// Vanilla `EnvironmentAttributes.BABY_VILLAGER_ACTIVITY`.
    BabyVillagerActivity,
}

impl ScheduleAttribute {
    /// Returns the name the `villager_schedule` timeline tracks this under.
    #[must_use]
    pub const fn attribute_name(self) -> &'static str {
        match self {
            Self::VillagerActivity => "minecraft:gameplay/villager_activity",
            Self::BabyVillagerActivity => "minecraft:gameplay/baby_villager_activity",
        }
    }

    /// The value the attribute falls back to where no timeline covers it.
    ///
    /// Vanilla parity: the `defaultValue(Activity.IDLE)` both attributes are
    /// registered with.
    pub const DEFAULT_ACTIVITY: Activity = Activity::Idle;
}
