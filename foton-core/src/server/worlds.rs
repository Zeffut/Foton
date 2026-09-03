//! Domain-aware loaded world map.

use std::sync::Arc;

use foton_utils::{Identifier, locks::SyncRwLock};
use rustc_hash::FxHashMap;
use small_map::FxSmallMap;

use crate::config::{ResolvedDomainConfig, ResolvedWorldConfig};
use crate::world::World;

pub(crate) const OVERWORLD_WORLD_NAME: &str = "overworld";
pub(crate) const NETHER_WORLD_NAME: &str = "the_nether";
pub(crate) const END_WORLD_NAME: &str = "the_end";

/// Why removing a loaded world was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldRemovalError {
    /// The identifier is not currently loaded.
    NotLoaded,
    /// A domain default world must remain loaded.
    DefaultWorld,
    /// Players must be moved out before the world can be detached.
    PlayersPresent(usize),
}

/// An owned read-only view of a loaded world for cross-phase server work.
///
/// Keeping both the stable identifier and the Arc together lets consumers
/// retain a world snapshot while the loaded-world map is being mutated at a
/// safe-point.
#[derive(Clone)]
pub struct WorldMapSnapshot {
    key: Identifier,
    world: Arc<World>,
}

impl WorldMapSnapshot {
    /// Creates a snapshot from a loaded world.
    #[must_use]
    pub fn new(world: &Arc<World>) -> Self {
        Self {
            key: world.key.clone(),
            world: Arc::clone(world),
        }
    }

    /// Returns the stable loaded-world identifier.
    #[must_use]
    pub const fn key(&self) -> &Identifier {
        &self.key
    }

    /// Returns the retained world reference.
    #[must_use]
    pub const fn world(&self) -> &Arc<World> {
        &self.world
    }
}

impl From<&Arc<World>> for WorldMapSnapshot {
    fn from(world: &Arc<World>) -> Self {
        Self::new(world)
    }
}

/// Loaded worlds plus domain defaults.
pub struct WorldMap {
    worlds: SyncRwLock<FxSmallMap<8, Identifier, Arc<World>>>,
    default_domain: String,
    default_worlds: FxHashMap<String, Identifier>,
    nether_portal_targets: FxHashMap<Identifier, Identifier>,
    end_portal_targets: FxHashMap<Identifier, Identifier>,
}

impl WorldMap {
    /// Creates a world map from resolved domain config.
    #[must_use]
    pub fn new(
        default_domain: String,
        domains: &[ResolvedDomainConfig],
        world_configs: &[ResolvedWorldConfig],
    ) -> Self {
        let mut default_worlds = FxHashMap::default();
        for domain in domains {
            default_worlds.insert(domain.name.clone(), domain.default_world.clone());
        }
        let mut nether_portal_targets = FxHashMap::default();
        let mut end_portal_targets = FxHashMap::default();
        for world in world_configs {
            if let Some(target) = &world.nether_portal_target {
                nether_portal_targets.insert(world.key.clone(), target.clone());
            }
            if let Some(target) = &world.end_portal_target {
                end_portal_targets.insert(world.key.clone(), target.clone());
            }
        }
        Self {
            worlds: SyncRwLock::new(FxSmallMap::default()),
            default_domain,
            default_worlds,
            nether_portal_targets,
            end_portal_targets,
        }
    }

    /// Inserts a loaded world without replacing an existing entry.
    pub fn insert(&self, key: Identifier, world: Arc<World>) -> Result<(), String> {
        if !Identifier::validate(&key.namespace, &key.path)
            || key.namespace.is_empty()
            || key.path.is_empty()
            || key.path.contains('/')
        {
            return Err(format!("invalid world identifier {key}"));
        }
        if world.key != key {
            return Err(format!(
                "world key mismatch: map requested {key}, world is {}",
                world.key
            ));
        }
        if !self.has_domain(&key.namespace) {
            return Err(format!("unknown world domain {}", key.namespace));
        }
        let mut worlds = self.worlds.write();
        if worlds.get(&key).is_some() {
            return Err(format!("world {key} is already loaded"));
        }
        worlds.insert(key, world);
        Ok(())
    }

    /// Returns a world by loaded world identifier.
    #[must_use]
    pub fn get(&self, key: &Identifier) -> Option<Arc<World>> {
        self.worlds.read().get(key).cloned()
    }

    /// Returns an owned world reference suitable for cross-phase work.
    #[must_use]
    pub fn get_owned(&self, key: &Identifier) -> Option<Arc<World>> {
        self.get(key)
    }

    /// Removes a loaded world after checking lifecycle invariants.
    ///
    /// A world cannot be detached while it is a configured domain default or
    /// while players still belong to it. The caller remains responsible for
    /// persistence and shutting down world workers before invoking this method.
    pub fn remove(&self, key: &Identifier) -> Result<Arc<World>, WorldRemovalError> {
        let Some(world) = self.get(key) else {
            return Err(WorldRemovalError::NotLoaded);
        };
        if self.default_worlds.values().any(|default| default == key) {
            return Err(WorldRemovalError::DefaultWorld);
        }
        let player_count = world.players.len();
        if player_count != 0 {
            return Err(WorldRemovalError::PlayersPresent(player_count));
        }
        self.worlds
            .write()
            .remove(key)
            .ok_or(WorldRemovalError::NotLoaded)
    }

    /// Iterates loaded world values.
    pub fn values(&self) -> Vec<Arc<World>> {
        self.worlds.read().values().cloned().collect()
    }

    /// Captures owned snapshots of all currently loaded worlds.
    #[must_use]
    pub fn snapshots(&self) -> Vec<WorldMapSnapshot> {
        self.worlds
            .read()
            .values()
            .map(WorldMapSnapshot::new)
            .collect()
    }

    /// Iterates loaded world keys.
    pub fn keys(&self) -> Vec<Identifier> {
        self.worlds.read().keys().cloned().collect()
    }

    /// Captures owned identifiers of all currently loaded worlds.
    #[must_use]
    pub fn key_snapshots(&self) -> Vec<Identifier> {
        self.worlds.read().keys().cloned().collect()
    }

    /// Captures owned key/world pairs for all currently loaded worlds.
    #[must_use]
    pub fn entry_snapshots(&self) -> Vec<(Identifier, Arc<World>)> {
        self.worlds
            .read()
            .iter()
            .map(|(key, world)| (key.clone(), Arc::clone(world)))
            .collect()
    }

    /// Returns number of loaded worlds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.worlds.read().len()
    }

    /// Returns whether there are no loaded worlds.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.worlds.read().is_empty()
    }

    /// Returns the default domain name.
    #[must_use]
    pub fn default_domain(&self) -> &str {
        &self.default_domain
    }

    /// Returns whether a domain exists.
    #[must_use]
    pub fn has_domain(&self, domain: &str) -> bool {
        self.default_worlds.contains_key(domain)
    }

    /// Iterates domain names.
    pub fn domain_names(&self) -> impl Iterator<Item = &str> {
        self.default_worlds.keys().map(String::as_str)
    }

    /// Returns a domain's default world.
    #[must_use]
    pub fn default_world(&self, domain: &str) -> Option<Arc<World>> {
        self.default_worlds
            .get(domain)
            .and_then(|key| self.worlds.read().get(key).cloned())
    }

    /// Returns the server default world.
    #[must_use]
    pub fn server_default_world(&self) -> Option<Arc<World>> {
        self.default_world(self.default_domain())
    }

    /// Returns loaded worlds in the given domain.
    #[must_use]
    pub fn worlds_in_domain(&self, domain: &str) -> Vec<Arc<World>> {
        self.worlds
            .read()
            .values()
            .filter(|world| world.domain() == domain)
            .cloned()
            .collect()
    }

    /// Resolves a conventional portal target name in the source world's domain.
    #[must_use]
    pub fn resolve_portal_target(
        &self,
        source_world: &World,
        target_world_name: &str,
    ) -> Option<Arc<World>> {
        let key = Identifier::new(
            source_world.domain().to_owned(),
            target_world_name.to_owned(),
        );
        self.get_owned(&key)
    }

    /// Resolves the vanilla Nether portal target in the source world's domain.
    #[must_use]
    pub fn resolve_nether_portal_target(&self, source_world: &World) -> Option<Arc<World>> {
        if let Some(target) = self.nether_portal_targets.get(&source_world.key) {
            return self.get_owned(target);
        }

        self.resolve_portal_target(
            source_world,
            nether_portal_target_world_name(source_world.key.path.as_ref()),
        )
    }

    /// Resolves the vanilla End portal target for non-End source worlds.
    ///
    /// End-to-respawn-world transitions depend on the source world's respawn data,
    /// so that branch is intentionally left to the destination calculator.
    #[must_use]
    pub fn resolve_end_entry_portal_target(&self, source_world: &World) -> Option<Arc<World>> {
        if let Some(target) = self.end_portal_targets.get(&source_world.key) {
            return self.get(target);
        }

        end_entry_portal_target_world_name(source_world.key.path.as_ref())
            .and_then(|target| self.resolve_portal_target(source_world, target))
    }
}

fn nether_portal_target_world_name(source_world_name: &str) -> &'static str {
    if source_world_name == NETHER_WORLD_NAME {
        OVERWORLD_WORLD_NAME
    } else {
        NETHER_WORLD_NAME
    }
}

fn end_entry_portal_target_world_name(source_world_name: &str) -> Option<&'static str> {
    if source_world_name == END_WORLD_NAME {
        None
    } else {
        Some(END_WORLD_NAME)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        WorldMap, WorldMapSnapshot, WorldRemovalError, end_entry_portal_target_world_name,
        nether_portal_target_world_name,
    };
    use crate::config::ResolvedDomainConfig;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world_in_domain};
    use foton_utils::Identifier;
    use std::sync::Arc;

    #[test]
    fn nether_portal_target_names_follow_vanilla_level_keys() {
        assert_eq!(nether_portal_target_world_name("overworld"), "the_nether");
        assert_eq!(nether_portal_target_world_name("the_end"), "the_nether");
        assert_eq!(nether_portal_target_world_name("the_nether"), "overworld");
    }

    #[test]
    fn end_entry_portal_target_name_is_only_for_non_end_sources() {
        assert_eq!(
            end_entry_portal_target_world_name("overworld"),
            Some("the_end")
        );
        assert_eq!(
            end_entry_portal_target_world_name("the_nether"),
            Some("the_end")
        );
        assert_eq!(end_entry_portal_target_world_name("the_end"), None);
    }
    fn domain(name: &str, default_world: Identifier) -> ResolvedDomainConfig {
        ResolvedDomainConfig {
            name: name.to_owned(),
            default_world,
            worlds: Vec::new(),
        }
    }

    #[test]
    fn snapshot_retains_world_and_stable_identifier() {
        let world = fresh_test_world_in_domain("main", "snapshot");
        let snapshot = WorldMapSnapshot::new(&world);

        assert_eq!(snapshot.key(), &world.key);
        assert!(Arc::ptr_eq(snapshot.world(), &world));
    }

    #[test]
    fn map_snapshots_are_owned_and_cover_loaded_worlds() {
        let world = fresh_test_world_in_domain("main", "snapshot_map");
        let key = world.key.clone();
        let worlds = WorldMap::new(
            "main".to_owned(),
            &[domain("main", Identifier::new("main", "spawn"))],
            &[],
        );
        assert!(worlds.insert(key.clone(), Arc::clone(&world)).is_ok());

        let snapshots = worlds.snapshots();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].key(), &key);
        assert!(Arc::ptr_eq(snapshots[0].world(), &world));
    }

    #[test]
    fn insert_rejects_duplicate_worlds_and_unknown_domains() {
        let world = fresh_test_world_in_domain("main", "arena");
        let key = world.key.clone();
        let worlds = WorldMap::new("main".to_owned(), &[], &[]);
        assert!(matches!(
            worlds.insert(key.clone(), Arc::clone(&world)),
            Err(error) if error.contains("unknown world domain")
        ));

        let worlds = WorldMap::new("main".to_owned(), &[domain("main", key.clone())], &[]);
        assert!(worlds.insert(key.clone(), Arc::clone(&world)).is_ok());
        assert!(matches!(
            worlds.insert(key, world),
            Err(error) if error.contains("already loaded")
        ));
    }

    #[test]
    fn insert_rejects_invalid_identifier_before_publishing() {
        let world = fresh_test_world_in_domain("main", "arena");
        let worlds = WorldMap::new("main".to_owned(), &[domain("main", world.key.clone())], &[]);
        let invalid = Identifier::new("main", "bad/name");
        assert!(matches!(
            worlds.insert(invalid, world),
            Err(error) if error.contains("invalid world identifier")
        ));
    }

    #[test]
    fn remove_rejects_unloaded_world() {
        let worlds = WorldMap::new("main".to_owned(), &[], &[]);
        let key = Identifier::new("main", "missing");
        assert!(matches!(
            worlds.remove(&key),
            Err(WorldRemovalError::NotLoaded)
        ));
    }

    #[test]
    fn remove_rejects_domain_default_world() {
        let world = fresh_test_world_in_domain("main", "spawn");
        let key = world.key.clone();
        let worlds = WorldMap::new("main".to_owned(), &[domain("main", key.clone())], &[]);
        assert!(worlds.insert(key.clone(), world).is_ok());
        assert!(matches!(
            worlds.remove(&key),
            Err(WorldRemovalError::DefaultWorld)
        ));
        assert!(worlds.get(&key).is_some());
    }

    #[test]
    fn remove_rejects_world_with_players() {
        let world = fresh_test_world_in_domain("main", "arena");
        let key = world.key.clone();
        let worlds = WorldMap::new(
            "main".to_owned(),
            &[domain("main", Identifier::new("main", "spawn"))],
            &[],
        );
        let player = TestPlayerBuilder::new(Arc::clone(&world), "player", 1).build();
        assert!(world.players.insert(player));
        assert!(worlds.insert(key.clone(), Arc::clone(&world)).is_ok());
        assert!(matches!(
            worlds.remove(&key),
            Err(WorldRemovalError::PlayersPresent(1))
        ));
        assert!(worlds.get(&key).is_some());
    }

    #[test]
    fn remove_detaches_empty_non_default_world() {
        let world = fresh_test_world_in_domain("main", "arena");
        let key = world.key.clone();
        let worlds = WorldMap::new(
            "main".to_owned(),
            &[domain("main", Identifier::new("main", "spawn"))],
            &[],
        );
        assert!(worlds.insert(key.clone(), world).is_ok());
        assert!(worlds.remove(&key).is_ok());
        assert!(worlds.get(&key).is_none());
    }
}
