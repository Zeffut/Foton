//! Vanilla `Activity`.

/// One named slice of a brain's behavior schedule.
///
/// Vanilla parity: `net.minecraft.world.entity.schedule.Activity`.
///
/// Vanilla registers these into `BuiltInRegistries.ACTIVITY`, but the registry
/// is a hardcoded Java list -- no datapack can add to it, no packet carries an
/// activity id and nothing writes one to disk. `SteelExtractor` emits no
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
}
