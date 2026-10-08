//! World-level data: `level.dat` and the saved-data files beside it.
//!
//! Vanilla 26.x split the old `level.dat` `Data` compound: the seed, game rules,
//! weather, clocks and per-dimension world border now live in
//! `data/minecraft/*.dat`. Foton keeps one `level.toml` per world, so a source
//! world's values are gathered here and written once per imported dimension.

use std::fs;
use std::io::{Cursor, ErrorKind, Read as _};
use std::path::Path;

use flate2::read::GzDecoder;
use foton_registry::{REGISTRY, RegistryExt as _};
use foton_utils::types::Difficulty;
use foton_utils::version::WORLD_VERSION;
use foton_utils::{BlockPos, Identifier};
use simdnbt::owned::{Nbt, NbtCompound, NbtTag, read};

use super::ImportError;
use crate::level_data::{LevelData, RespawnData, SpawnPoint, WeatherState, WorldBorderData};

/// Everything the importer carries over from the world's own data files.
#[derive(Debug, Default)]
pub(super) struct SourceLevel {
    pub(super) seed: Option<i64>,
    game_time: i64,
    difficulty: Option<(Difficulty, bool)>,
    spawn: Option<SourceSpawn>,
    weather: Option<WeatherState>,
    game_rules: Vec<(String, serde_json::Value)>,
    clocks: Vec<SourceClock>,
}

#[derive(Debug)]
struct SourceSpawn {
    dimension: Identifier,
    pos: [i32; 3],
    yaw: f32,
    pitch: f32,
}

#[derive(Debug)]
struct SourceClock {
    key: Identifier,
    total_ticks: i64,
    paused: Option<bool>,
    rate: Option<f32>,
}

fn read_gzip_nbt(path: &Path) -> Result<Option<NbtCompound>, ImportError> {
    let compressed = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(ImportError::io(path, error)),
    };
    let mut bytes = Vec::new();
    GzDecoder::new(compressed.as_slice())
        .read_to_end(&mut bytes)
        .map_err(|error| ImportError::io(path, error))?;
    match read(&mut Cursor::new(bytes.as_slice())) {
        Ok(Nbt::Some(root)) => Ok(Some(root.as_compound())),
        Ok(Nbt::None) => Ok(None),
        Err(error) => Err(ImportError::Level(format!(
            "{} is not valid NBT: {error}",
            path.display()
        ))),
    }
}

/// The `data` compound of a `data/minecraft/<name>.dat` file, if it exists.
fn read_saved_data(root: &Path, name: &str) -> Result<Option<NbtCompound>, ImportError> {
    let path = root
        .join("data")
        .join("minecraft")
        .join(format!("{name}.dat"));
    Ok(
        read_gzip_nbt(&path)?.and_then(|mut file| match file.take("data") {
            Some(NbtTag::Compound(data)) => Some(data),
            _ => None,
        }),
    )
}

fn rule_value(tag: &NbtTag) -> Option<serde_json::Value> {
    match tag {
        NbtTag::Byte(value) => Some(serde_json::Value::Bool(*value != 0)),
        NbtTag::Short(value) => Some((*value).into()),
        NbtTag::Int(value) => Some((*value).into()),
        NbtTag::Long(value) => Some((*value).into()),
        NbtTag::Float(value) => serde_json::Number::from_f64(f64::from(*value)).map(Into::into),
        NbtTag::Double(value) => serde_json::Number::from_f64(*value).map(Into::into),
        _ => None,
    }
}

fn parse_difficulty(name: &str) -> Option<Difficulty> {
    match name {
        "peaceful" => Some(Difficulty::Peaceful),
        "easy" => Some(Difficulty::Easy),
        "normal" => Some(Difficulty::Normal),
        "hard" => Some(Difficulty::Hard),
        _ => None,
    }
}

impl SourceLevel {
    /// Reads the world's data, or `None` if the folder has no `level.dat`.
    ///
    /// # Errors
    /// [`ImportError::DataVersion`] if `level.dat` was written by another
    /// Minecraft version.
    pub(super) fn read(root: &Path) -> Result<Option<Self>, ImportError> {
        let Some(level) = read_gzip_nbt(&root.join("level.dat"))? else {
            return Ok(None);
        };
        let data = level
            .compound("Data")
            .ok_or_else(|| ImportError::Level("level.dat has no Data compound".to_owned()))?;
        let version = data.int("DataVersion");
        if version != Some(WORLD_VERSION) {
            return Err(ImportError::DataVersion {
                what: "level.dat".to_owned(),
                found: version,
                skippable: false,
            });
        }

        let mut source = Self {
            game_time: data.long("Time").unwrap_or(0),
            ..Self::default()
        };
        source.difficulty = data.compound("difficulty_settings").and_then(|settings| {
            let difficulty = parse_difficulty(&settings.string("difficulty")?.to_str())?;
            Some((
                difficulty,
                settings.byte("locked").is_some_and(|flag| flag != 0),
            ))
        });
        source.spawn = data.compound("spawn").and_then(|spawn| {
            let pos = spawn.int_array("pos").filter(|pos| pos.len() == 3)?;
            Some(SourceSpawn {
                dimension: spawn.string("dimension")?.to_str().parse().ok()?,
                pos: [pos[0], pos[1], pos[2]],
                yaw: spawn.float("yaw").unwrap_or(0.0),
                pitch: spawn.float("pitch").unwrap_or(0.0),
            })
        });

        source.seed =
            read_saved_data(root, "world_gen_settings")?.and_then(|settings| settings.long("seed"));
        source.weather = read_saved_data(root, "weather")?.map(|weather| WeatherState {
            raining: weather.byte("raining").is_some_and(|flag| flag != 0),
            rain_time: weather.int("rain_time").unwrap_or(0),
            thundering: weather.byte("thundering").is_some_and(|flag| flag != 0),
            thunder_time: weather.int("thunder_time").unwrap_or(0),
            clear_weather_time: weather.int("clear_weather_time").unwrap_or(0),
        });
        if let Some(rules) = read_saved_data(root, "game_rules")? {
            source.game_rules = rules
                .iter()
                .filter_map(|(name, tag)| {
                    let name = name.to_str();
                    let name = name.strip_prefix("minecraft:").unwrap_or(&name);
                    Some((name.to_owned(), rule_value(tag)?))
                })
                .collect();
        }
        if let Some(clocks) = read_saved_data(root, "world_clocks")? {
            source.clocks = clocks
                .iter()
                .filter_map(|(name, tag)| {
                    let clock = tag.compound()?;
                    Some(SourceClock {
                        key: name.to_str().parse().ok()?,
                        total_ticks: clock.long("total_ticks").unwrap_or(0),
                        paused: clock.byte("paused").map(|flag| flag != 0),
                        rate: clock.float("rate"),
                    })
                })
                .collect();
        }
        Ok(Some(source))
    }

    /// Builds the `level.toml` content for one imported dimension.
    ///
    /// `generation` stays unset: the first start adopts the configured
    /// generator settings, which is how every other pre-existing world is
    /// treated.
    pub(super) fn level_data(
        &self,
        seed: i64,
        dimension: &Identifier,
        border: Option<WorldBorderData>,
        is_spawn_world: bool,
    ) -> LevelData {
        let (difficulty, locked) = self.difficulty.unwrap_or_default();
        let mut data = LevelData::new_with_seed_and_difficulty(seed, difficulty);
        data.difficulty_locked = locked;
        data.game_time = self.game_time;
        data.initialized = true;
        if let Some(weather) = &self.weather {
            data.weather = weather.clone();
        }
        if let Some(border) = border {
            data.world_border = border;
        }
        data.game_rules = self.game_rules.iter().cloned().collect();

        for clock in &self.clocks {
            let Some(registered) = REGISTRY.world_clocks.by_key(&clock.key) else {
                continue;
            };
            let _ = data
                .world_clocks
                .set_total_ticks(registered, clock.total_ticks);
            if let Some(paused) = clock.paused {
                let _ = data.world_clocks.set_paused(registered, paused);
            }
            if let Some(rate) = clock.rate {
                let _ = data.world_clocks.set_rate(registered, rate);
            }
        }

        if let Some(spawn) = self
            .spawn
            .as_ref()
            .filter(|spawn| &spawn.dimension == dimension)
        {
            data.spawn = SpawnPoint {
                x: spawn.pos[0],
                y: spawn.pos[1],
                z: spawn.pos[2],
                angle: spawn.yaw,
            };
            if is_spawn_world {
                data.respawn = Some(RespawnData::of(
                    spawn.dimension.clone(),
                    BlockPos::new(spawn.pos[0], spawn.pos[1], spawn.pos[2]),
                    spawn.yaw,
                    spawn.pitch,
                ));
            }
        }
        data
    }
}

/// Reads a dimension's world border, which 26.x stores per dimension.
pub(super) fn read_world_border(
    dimension_dir: &Path,
) -> Result<Option<WorldBorderData>, ImportError> {
    let Some(border) = read_saved_data(dimension_dir, "world_border")? else {
        return Ok(None);
    };
    let defaults = WorldBorderData::default();
    Ok(Some(WorldBorderData {
        center_x: border.double("center_x").unwrap_or(defaults.center_x),
        center_z: border.double("center_z").unwrap_or(defaults.center_z),
        damage_per_block: border
            .double("damage_per_block")
            .unwrap_or(defaults.damage_per_block),
        safe_zone: border.double("safe_zone").unwrap_or(defaults.safe_zone),
        warning_blocks: border
            .int("warning_blocks")
            .unwrap_or(defaults.warning_blocks),
        warning_time: border.int("warning_time").unwrap_or(defaults.warning_time),
        size: border.double("size").unwrap_or(defaults.size),
        lerp_time: border.long("lerp_time").unwrap_or(defaults.lerp_time),
        lerp_target: border.double("lerp_target").unwrap_or(defaults.lerp_target),
    }))
}
