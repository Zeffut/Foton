//! The state behind one filled map.
//!
//! Vanilla parity: `net.minecraft.world.level.saveddata.maps.MapItemSavedData`.

use std::mem;
use std::sync::Arc;

use rustc_hash::FxHashMap;
use steel_protocol::packets::game::{CMapItemData, MapDecorationData, MapPatch};
use steel_registry::data_components::components::MapDecorations;
use steel_registry::map_decoration_type::MapDecorationTypeRef;
use steel_registry::vanilla_map_decoration_types as decoration_types;
use steel_utils::{BlockPos, Identifier};
use text_components::TextComponent;
use uuid::Uuid;

use crate::map::markers::{MapBanner, MapDecoration, MapFrame};
use crate::world::World;

/// Side of the square image a map draws, in pixels.
pub const MAP_SIZE: usize = 128;
/// Number of color bytes one map holds.
pub const MAP_COLOR_COUNT: usize = MAP_SIZE * MAP_SIZE;
/// Vanilla `MapItemSavedData.MAX_SCALE`.
pub const MAX_SCALE: u8 = 4;
/// Vanilla `MapItemSavedData.TRACKED_DECORATION_LIMIT`.
pub const TRACKED_DECORATION_LIMIT: i32 = 256;

/// The parts of a player that `MapItemSavedData` reads while ticking.
///
/// Vanilla reaches into the `Player` object directly. Steel snapshots the
/// fields instead, because the map is behind its own lock and cannot hold a
/// borrow of a player across a tick.
#[derive(Debug, Clone)]
pub struct MapPlayerState {
    /// Identity of the player, replacing vanilla's live player reference.
    pub uuid: Uuid,
    /// Vanilla `Player.getPlainTextName`, the key its decoration is filed under.
    pub name: String,
    /// Player position on the world's X axis.
    pub x: f64,
    /// Player position on the world's Z axis.
    pub z: f64,
    /// Player yaw, which becomes the arrow's facing.
    pub y_rot: f32,
    /// Key of the world the player stands in right now.
    pub dimension: Identifier,
    /// Whether the player still carries a stack matching the map being ticked.
    ///
    /// Vanilla parity: `player.getInventory().contains(mapMatcher(itemStack))`.
    pub holds_map: bool,
    /// Vanilla parity: `hasMapInvisibilityItemEquipped`.
    pub map_invisible: bool,
}

/// Resolves the players a map is already tracking.
///
/// Vanilla walks its `carriedBy` list of live `Player` references; Steel keeps
/// UUIDs and asks the caller to turn them back into players, so the map does
/// not need to reach the world.
pub trait MapPlayerSource {
    /// Returns the current state of a tracked player, or `None` once they are
    /// gone -- vanilla's `Entity.isRemoved`.
    fn holder(&self, uuid: Uuid) -> Option<MapPlayerState>;
}

/// One player the map is being sent to, and what they still need.
///
/// Vanilla parity: `MapItemSavedData.HoldingPlayer`.
#[derive(Debug)]
pub struct HoldingPlayer {
    /// Identity of the player this tracking state belongs to.
    pub uuid: Uuid,
    /// Last known name, kept so a decoration can still be removed for a player
    /// who has disconnected.
    name: String,
    dirty_data: bool,
    min_dirty_x: usize,
    min_dirty_y: usize,
    max_dirty_x: usize,
    max_dirty_y: usize,
    dirty_decorations: bool,
    tick: i32,
    /// Vanilla parity: the column stripe `MapItem.update` redraws this tick.
    pub step: i32,
}

impl HoldingPlayer {
    const fn new(uuid: Uuid, name: String) -> Self {
        Self {
            uuid,
            name,
            dirty_data: true,
            min_dirty_x: 0,
            min_dirty_y: 0,
            max_dirty_x: MAP_SIZE - 1,
            max_dirty_y: MAP_SIZE - 1,
            dirty_decorations: true,
            tick: 0,
            step: 0,
        }
    }

    fn mark_colors_dirty(&mut self, x: usize, y: usize) {
        if self.dirty_data {
            self.min_dirty_x = self.min_dirty_x.min(x);
            self.min_dirty_y = self.min_dirty_y.min(y);
            self.max_dirty_x = self.max_dirty_x.max(x);
            self.max_dirty_y = self.max_dirty_y.max(y);
        } else {
            self.dirty_data = true;
            self.min_dirty_x = x;
            self.min_dirty_y = y;
            self.max_dirty_x = x;
            self.max_dirty_y = y;
        }
    }

    /// Consumes the dirty rectangle, returning `(start_x, start_y, width, height)`.
    const fn take_dirty_rect(&mut self) -> Option<(usize, usize, usize, usize)> {
        if !self.dirty_data {
            return None;
        }
        self.dirty_data = false;
        Some((
            self.min_dirty_x,
            self.min_dirty_y,
            self.max_dirty_x + 1 - self.min_dirty_x,
            self.max_dirty_y + 1 - self.min_dirty_y,
        ))
    }

    /// Vanilla parity: `this.dirtyDecorations && this.tick++ % 5 == 0`, whose
    /// short-circuit means the counter only advances while there is something
    /// to send.
    const fn take_dirty_decorations(&mut self) -> bool {
        if !self.dirty_decorations {
            return false;
        }
        let due = self.tick % 5 == 0;
        self.tick = self.tick.wrapping_add(1);
        if due {
            self.dirty_decorations = false;
        }
        due
    }
}

/// Everything one filled map remembers.
#[derive(Debug)]
pub struct MapItemSavedData {
    /// World X coordinate the map's center pixel sits over.
    pub center_x: i32,
    /// World Z coordinate the map's center pixel sits over.
    pub center_z: i32,
    /// Key of the world this map charts (`domain:world`).
    ///
    /// Vanilla parity: `ResourceKey<Level> dimension`.
    pub dimension: Identifier,
    /// Whether `dimension` is a Nether-type world.
    ///
    /// Vanilla tests `this.dimension == Level.NETHER` to decide whether player
    /// arrows spin. Steel's world keys are `domain:world` and carry no vanilla
    /// dimension identity, so the answer is recorded when the map is created.
    nether: bool,
    tracking_position: bool,
    unlimited_tracking: bool,
    /// Zoom level, zero to four; one pixel covers two-to-the-scale blocks.
    pub scale: u8,
    colors: Box<[u8; MAP_COLOR_COUNT]>,
    /// Whether the map has been locked in a cartography table.
    pub locked: bool,
    carried_by: Vec<HoldingPlayer>,
    banner_markers: FxHashMap<String, MapBanner>,
    /// Insertion-ordered, matching vanilla's `LinkedHashMap`: the order is the
    /// order the client draws the icons in.
    decorations: Vec<(String, MapDecoration)>,
    frame_markers: FxHashMap<String, MapFrame>,
    tracked_decoration_count: i32,
    dirty: bool,
}

impl MapItemSavedData {
    #[expect(
        clippy::too_many_arguments,
        clippy::fn_params_excessive_bools,
        reason = "mirrors the field list of vanilla's private constructor"
    )]
    fn empty(
        center_x: i32,
        center_z: i32,
        scale: u8,
        tracking_position: bool,
        unlimited_tracking: bool,
        locked: bool,
        dimension: Identifier,
        nether: bool,
    ) -> Self {
        Self {
            center_x,
            center_z,
            dimension,
            nether,
            tracking_position,
            unlimited_tracking,
            scale: scale.min(MAX_SCALE),
            colors: Box::new([0; MAP_COLOR_COUNT]),
            locked,
            carried_by: Vec::new(),
            banner_markers: FxHashMap::default(),
            decorations: Vec::new(),
            frame_markers: FxHashMap::default(),
            tracked_decoration_count: 0,
            dirty: true,
        }
    }

    /// Vanilla parity: `MapItemSavedData.createFresh`.
    #[must_use]
    pub fn create_fresh(
        origin_x: f64,
        origin_z: f64,
        scale: u8,
        tracking_position: bool,
        unlimited_tracking: bool,
        dimension: Identifier,
        nether: bool,
    ) -> Self {
        let scale = scale.min(MAX_SCALE);
        let size = i32::try_from(MAP_SIZE).unwrap_or(128) * (1 << scale);
        let area_x = ((origin_x + 64.0) / f64::from(size)).floor() as i32;
        let area_z = ((origin_z + 64.0) / f64::from(size)).floor() as i32;
        Self::empty(
            area_x * size + size / 2 - 64,
            area_z * size + size / 2 - 64,
            scale,
            tracking_position,
            unlimited_tracking,
            false,
            dimension,
            nether,
        )
    }

    /// Rebuilds a map read back from disk.
    #[expect(
        clippy::too_many_arguments,
        clippy::fn_params_excessive_bools,
        reason = "mirrors the field list vanilla's codec reads back"
    )]
    #[must_use]
    pub fn from_persisted(
        dimension: Identifier,
        nether: bool,
        center_x: i32,
        center_z: i32,
        scale: u8,
        colors: &[u8],
        tracking_position: bool,
        unlimited_tracking: bool,
        locked: bool,
        banners: Vec<MapBanner>,
        frames: Vec<MapFrame>,
    ) -> Self {
        let mut data = Self::empty(
            center_x,
            center_z,
            scale,
            tracking_position,
            unlimited_tracking,
            locked,
            dimension,
            nether,
        );
        // Vanilla keeps the blank array when the stored one is the wrong size.
        if colors.len() == MAP_COLOR_COUNT {
            data.colors.copy_from_slice(colors);
        }

        for banner in banners {
            let decoration = banner.decoration();
            let id = banner.id();
            let (x, z) = (f64::from(banner.pos.x()), f64::from(banner.pos.z()));
            let name = banner.name.clone();
            data.banner_markers.insert(id.clone(), banner);
            data.add_decoration(decoration, None, id, x, z, 180.0, name);
        }

        for frame in frames {
            let key = Self::frame_key(frame.entity_id);
            data.frame_markers.insert(frame.id(), frame);
            data.add_decoration(
                &decoration_types::FRAME,
                None,
                key,
                f64::from(frame.pos.x()),
                f64::from(frame.pos.z()),
                f64::from(frame.rotation),
                None,
            );
        }

        data.dirty = false;
        data
    }

    /// Vanilla parity: `MapItemSavedData.locked`.
    #[must_use]
    pub fn locked_copy(&self) -> Self {
        let mut copy = Self::empty(
            self.center_x,
            self.center_z,
            self.scale,
            self.tracking_position,
            self.unlimited_tracking,
            true,
            self.dimension.clone(),
            self.nether,
        );
        copy.banner_markers.clone_from(&self.banner_markers);
        copy.decorations.clone_from(&self.decorations);
        copy.tracked_decoration_count = self.tracked_decoration_count;
        copy.colors.copy_from_slice(self.colors.as_slice());
        copy
    }

    /// Vanilla parity: `MapItemSavedData.scaled`.
    #[must_use]
    pub fn scaled(&self) -> Self {
        Self::create_fresh(
            f64::from(self.center_x),
            f64::from(self.center_z),
            (self.scale + 1).min(MAX_SCALE),
            self.tracking_position,
            self.unlimited_tracking,
            self.dimension.clone(),
            self.nether,
        )
    }

    /// Vanilla parity: the trackingPosition flag.
    #[must_use]
    pub const fn tracking_position(&self) -> bool {
        self.tracking_position
    }

    /// Vanilla parity: the unlimitedTracking flag.
    #[must_use]
    pub const fn unlimited_tracking(&self) -> bool {
        self.unlimited_tracking
    }

    /// Whether this map charts a Nether-type world.
    #[must_use]
    pub const fn nether(&self) -> bool {
        self.nether
    }

    /// The packed color byte of every pixel, row-major.
    #[must_use]
    pub fn colors(&self) -> &[u8; MAP_COLOR_COUNT] {
        &self.colors
    }

    /// The banners marked on this map.
    pub fn banners(&self) -> impl Iterator<Item = &MapBanner> {
        self.banner_markers.values()
    }

    /// The item frames this map has been hung in.
    pub fn frames(&self) -> impl Iterator<Item = &MapFrame> {
        self.frame_markers.values()
    }

    /// The markers currently drawn, in the order the client draws them.
    pub fn decorations(&self) -> impl Iterator<Item = &MapDecoration> {
        self.decorations.iter().map(|(_, decoration)| decoration)
    }

    /// Whether this map has changed since it was last written to disk.
    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Clears the dirty flag after a successful write.
    pub const fn mark_saved(&mut self) {
        self.dirty = false;
    }

    /// Vanilla parity: `SavedData.setDirty`.
    pub const fn set_dirty(&mut self) {
        self.dirty = true;
    }

    /// Vanilla parity: `MapItemSavedData.isExplorationMap`.
    #[must_use]
    pub fn is_exploration_map(&self) -> bool {
        self.decorations
            .iter()
            .any(|(_, decoration)| decoration.decoration_type.exploration_map_element)
    }

    /// Vanilla parity: `MapItemSavedData.isTrackedCountOverLimit`.
    #[must_use]
    pub const fn is_tracked_count_over_limit(&self, limit: i32) -> bool {
        self.tracked_decoration_count > limit
    }

    /// Vanilla parity: `MapItemSavedData.setColor`.
    pub fn set_color(&mut self, x: usize, y: usize, color: u8) {
        self.colors[x + y * MAP_SIZE] = color;
        self.dirty = true;
        for holder in &mut self.carried_by {
            holder.mark_colors_dirty(x, y);
        }
    }

    /// Vanilla parity: `MapItemSavedData.updateColor`, whose return value tells
    /// `MapItem.update` whether the column stripe found anything new.
    pub fn update_color(&mut self, x: usize, y: usize, color: u8) -> bool {
        if self.colors[x + y * MAP_SIZE] == color {
            return false;
        }
        self.set_color(x, y, color);
        true
    }

    /// Vanilla parity: `MapItemSavedData.getHoldingPlayer`.
    ///
    /// # Panics
    /// Never in practice: the holder is pushed immediately before it is read
    /// back, so the vector cannot be empty at that point.
    pub fn holding_player_mut(&mut self, uuid: Uuid, name: &str) -> &mut HoldingPlayer {
        if let Some(index) = self
            .carried_by
            .iter()
            .position(|holder| holder.uuid == uuid)
        {
            self.carried_by[index].name.clear();
            self.carried_by[index].name.push_str(name);
            return &mut self.carried_by[index];
        }
        self.carried_by
            .push(HoldingPlayer::new(uuid, name.to_owned()));
        self.carried_by
            .last_mut()
            .expect("a holder was just pushed")
    }

    /// Vanilla parity: `MapItemSavedData.getUpdatePacket`, including the
    /// once-every-five-ticks throttle on the decoration list.
    pub fn update_packet(&mut self, map_id: i32, uuid: Uuid) -> Option<CMapItemData> {
        let index = self
            .carried_by
            .iter()
            .position(|holder| holder.uuid == uuid)?;
        let holder = &mut self.carried_by[index];
        let rect = holder.take_dirty_rect();
        let send_decorations = holder.take_dirty_decorations();
        if rect.is_none() && !send_decorations {
            return None;
        }

        let decorations = send_decorations.then(|| {
            self.decorations
                .iter()
                .map(|(_, decoration)| MapDecorationData {
                    decoration_type: decoration.decoration_type,
                    x: decoration.x,
                    y: decoration.y,
                    rot: decoration.rot,
                    name: decoration.name.clone(),
                })
                .collect()
        });

        Some(CMapItemData {
            map_id,
            scale: self.scale,
            locked: self.locked,
            decorations,
            color_patch: rect.map(|rect| self.create_patch(rect)),
        })
    }

    /// Vanilla parity: `HoldingPlayer.createPatch`.
    fn create_patch(
        &self,
        (start_x, start_y, width, height): (usize, usize, usize, usize),
    ) -> MapPatch {
        let mut map_colors = vec![0u8; width * height];
        for x in 0..width {
            for y in 0..height {
                map_colors[x + y * width] = self.colors[start_x + x + (start_y + y) * MAP_SIZE];
            }
        }
        MapPatch {
            start_x: start_x as u8,
            start_y: start_y as u8,
            width: width as u8,
            height: height as u8,
            map_colors,
        }
    }

    /// Vanilla parity: `MapItemSavedData.tickCarriedBy`.
    ///
    /// Deviation: vanilla removes from `carriedBy` while walking it with an
    /// index and never steps back, so a departing holder can shadow the next
    /// one for a tick. Steel re-examines the shifted element instead.
    pub fn tick_carried_by(
        &mut self,
        ticking: &MapPlayerState,
        source: &dyn MapPlayerSource,
        frame: Option<MapFrame>,
        static_decorations: &MapDecorations,
        game_time: i64,
    ) {
        self.holding_player_mut(ticking.uuid, &ticking.name);

        if !ticking.holds_map {
            self.remove_decoration(&ticking.name);
        }

        let mut index = 0;
        while index < self.carried_by.len() {
            let uuid = self.carried_by[index].uuid;
            let state = if uuid == ticking.uuid {
                Some(ticking.clone())
            } else {
                source.holder(uuid)
            };

            let Some(state) = state else {
                let holder = self.carried_by.remove(index);
                self.remove_decoration(&holder.name);
                continue;
            };
            self.carried_by[index].name.clone_from(&state.name);

            if frame.is_none() && !state.holds_map {
                self.carried_by.remove(index);
                self.remove_decoration(&state.name);
                continue;
            }

            if frame.is_none() && state.dimension == self.dimension && self.tracking_position {
                self.add_decoration(
                    &decoration_types::PLAYER,
                    Some(game_time),
                    state.name.clone(),
                    state.x,
                    state.z,
                    f64::from(state.y_rot),
                    None,
                );
            }

            if uuid != ticking.uuid && state.map_invisible {
                self.remove_decoration(&state.name);
            }

            index += 1;
        }

        if let Some(frame) = frame
            && self.tracking_position
        {
            let existing = self
                .frame_markers
                .get(&MapFrame::frame_id(frame.pos))
                .copied();
            if let Some(existing) = existing
                && existing.entity_id != frame.entity_id
            {
                self.remove_decoration(&Self::frame_key(existing.entity_id));
            }

            self.add_decoration(
                &decoration_types::FRAME,
                Some(game_time),
                Self::frame_key(frame.entity_id),
                f64::from(frame.pos.x()),
                f64::from(frame.pos.z()),
                f64::from(frame.rotation),
                None,
            );
            if self.frame_markers.insert(frame.id(), frame) != Some(frame) {
                self.dirty = true;
            }
        }

        for (id, entry) in static_decorations.decorations() {
            if self.decoration_index(id).is_some() {
                continue;
            }
            self.add_decoration(
                entry.decoration_type().value(),
                Some(game_time),
                id.clone(),
                entry.x(),
                entry.z(),
                f64::from(entry.rotation()),
                None,
            );
        }
    }

    /// Vanilla parity: `MapItemSavedData.toggleBanner`.
    pub fn toggle_banner(&mut self, world: &Arc<World>, pos: BlockPos) -> bool {
        let x_pos = f64::from(pos.x()) + 0.5;
        let z_pos = f64::from(pos.z()) + 0.5;
        let scaling = f64::from(1 << self.scale);
        let xd = (x_pos - f64::from(self.center_x)) / scaling;
        let yd = (z_pos - f64::from(self.center_z)) / scaling;
        if !(-63.0..=63.0).contains(&xd) || !(-63.0..=63.0).contains(&yd) {
            return false;
        }

        let Some(banner) = MapBanner::from_world(world, pos) else {
            return false;
        };
        let id = banner.id();

        if self.banner_markers.get(&id) == Some(&banner) {
            self.banner_markers.remove(&id);
            self.remove_decoration(&id);
            self.dirty = true;
            return true;
        }

        if self.is_tracked_count_over_limit(TRACKED_DECORATION_LIMIT) {
            return false;
        }

        let decoration = banner.decoration();
        let name = banner.name.clone();
        self.banner_markers.insert(id.clone(), banner);
        self.add_decoration(
            decoration,
            Some(world.game_time()),
            id,
            x_pos,
            z_pos,
            180.0,
            name,
        );
        self.dirty = true;
        true
    }

    /// Vanilla parity: `MapItemSavedData.checkBanners`.
    pub fn check_banners(&mut self, world: &Arc<World>, x: i32, z: i32) {
        let stale: Vec<String> = self
            .banner_markers
            .values()
            .filter(|banner| banner.pos.x() == x && banner.pos.z() == z)
            .filter(|banner| MapBanner::from_world(world, banner.pos).as_ref() != Some(*banner))
            .map(MapBanner::id)
            .collect();

        for id in stale {
            self.banner_markers.remove(&id);
            self.remove_decoration(&id);
            self.dirty = true;
        }
    }

    /// Vanilla parity: `MapItemSavedData.removedFromFrame`.
    pub fn removed_from_frame(&mut self, pos: BlockPos, entity_id: i32) {
        self.remove_decoration(&Self::frame_key(entity_id));
        self.frame_markers.remove(&MapFrame::frame_id(pos));
        self.dirty = true;
    }

    /// Vanilla parity: `MapItemSavedData.getFrameKey`.
    #[must_use]
    pub fn frame_key(entity_id: i32) -> String {
        format!("frame-{entity_id}")
    }

    fn decoration_index(&self, key: &str) -> Option<usize> {
        self.decorations.iter().position(|(id, _)| id == key)
    }

    /// Vanilla parity: `MapItemSavedData.removeDecoration`.
    fn remove_decoration(&mut self, key: &str) {
        if let Some(index) = self.decoration_index(key) {
            let (_, decoration) = self.decorations.remove(index);
            if decoration.decoration_type.track_count {
                self.tracked_decoration_count -= 1;
            }
        }
        self.set_decorations_dirty();
    }

    fn set_decorations_dirty(&mut self) {
        for holder in &mut self.carried_by {
            holder.dirty_decorations = true;
        }
    }

    /// Vanilla parity: `MapItemSavedData.addDecoration`.
    ///
    /// `game_time` is `None` for the decorations restored from disk, matching
    /// vanilla's `null` level there.
    #[expect(
        clippy::too_many_arguments,
        reason = "mirrors the parameter list of vanilla's addDecoration"
    )]
    fn add_decoration(
        &mut self,
        decoration_type: MapDecorationTypeRef,
        game_time: Option<i64>,
        key: String,
        x_pos: f64,
        z_pos: f64,
        y_rot: f64,
        name: Option<TextComponent>,
    ) {
        let scaling = f64::from(1 << self.scale);
        let x_delta = ((x_pos - f64::from(self.center_x)) / scaling) as f32;
        let y_delta = ((z_pos - f64::from(self.center_z)) / scaling) as f32;
        let Some((decoration_type, x, y, rot)) =
            self.decoration_location(decoration_type, game_time, y_rot, x_delta, y_delta)
        else {
            self.remove_decoration(&key);
            return;
        };

        let decoration = MapDecoration::new(decoration_type, x, y, rot, name);
        match self.decoration_index(&key) {
            Some(index) if self.decorations[index].1 == decoration => {}
            Some(index) => {
                let previous = mem::replace(&mut self.decorations[index].1, decoration);
                if previous.decoration_type.track_count {
                    self.tracked_decoration_count -= 1;
                }
                if decoration_type.track_count {
                    self.tracked_decoration_count += 1;
                }
                self.set_decorations_dirty();
            }
            None => {
                self.decorations.push((key, decoration));
                if decoration_type.track_count {
                    self.tracked_decoration_count += 1;
                }
                self.set_decorations_dirty();
            }
        }
    }

    /// Vanilla parity: `MapItemSavedData.calculateDecorationLocationAndType`.
    fn decoration_location(
        &self,
        decoration_type: MapDecorationTypeRef,
        game_time: Option<i64>,
        y_rot: f64,
        x_delta: f32,
        y_delta: f32,
    ) -> Option<(MapDecorationTypeRef, i8, i8, i8)> {
        let x = clamp_map_coordinate(x_delta);
        let y = clamp_map_coordinate(y_delta);

        if decoration_type.key == decoration_types::PLAYER.key {
            if is_inside_map(x_delta, y_delta) {
                return Some((
                    decoration_type,
                    x,
                    y,
                    self.calculate_rotation(game_time, y_rot),
                ));
            }
            let outside = self.decoration_type_for_player_outside_map(x_delta, y_delta)?;
            return Some((outside, x, y, 0));
        }

        if !is_inside_map(x_delta, y_delta) && !self.unlimited_tracking {
            return None;
        }
        Some((
            decoration_type,
            x,
            y,
            self.calculate_rotation(game_time, y_rot),
        ))
    }

    /// Vanilla parity: `MapItemSavedData.calculateRotation`.
    fn calculate_rotation(&self, game_time: Option<i64>, y_rot: f64) -> i8 {
        if let Some(game_time) = game_time
            && self.nether
        {
            let s = (game_time / 10) as i32;
            return ((s.wrapping_mul(s).wrapping_mul(34_187_121)).wrapping_add(s.wrapping_mul(121))
                >> 15
                & 15) as i8;
        }
        let adjusted = if y_rot < 0.0 {
            y_rot - 8.0
        } else {
            y_rot + 8.0
        };
        (adjusted * 16.0 / 360.0) as i8
    }

    /// Vanilla parity: `MapItemSavedData.decorationTypeForPlayerOutsideMap`.
    fn decoration_type_for_player_outside_map(
        &self,
        x_delta: f32,
        y_delta: f32,
    ) -> Option<MapDecorationTypeRef> {
        if x_delta.abs() < 320.0 && y_delta.abs() < 320.0 {
            return Some(&decoration_types::PLAYER_OFF_MAP);
        }
        self.unlimited_tracking
            .then_some(&decoration_types::PLAYER_OFF_LIMITS)
    }
}

/// Vanilla parity: `MapItemSavedData.isInsideMap`.
fn is_inside_map(x_delta: f32, y_delta: f32) -> bool {
    (-63.0..=63.0).contains(&x_delta) && (-63.0..=63.0).contains(&y_delta)
}

/// Vanilla parity: `MapItemSavedData.clampMapCoordinate`.
fn clamp_map_coordinate(delta: f32) -> i8 {
    if delta <= -63.0 {
        return -128;
    }
    if delta >= 63.0 {
        return 127;
    }
    (delta * 2.0 + 0.5) as i8
}
