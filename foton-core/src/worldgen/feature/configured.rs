use super::prelude::*;
use super::runner::FeatureDecorationRunner;
use crate::worldgen::template::{
    StructurePlaceSettings, StructureProcessorRandom, StructureTemplate,
};
use foton_registry::structure::LiquidSettingsData;
use foton_utils::BoundingBox;
use foton_worldgen::structure::{StructureBlockIgnore, StructureMirror};
use glam::IVec3;

struct ConfiguredFeaturePlaceContext<'a, L> {
    region: &'a L,
    registry: &'a Registry,
    random: &'a mut WorldgenRandom,
    origin: BlockPos,
    biome_zoom_seed: i64,
}

type ConfiguredFeaturePlacer<L> =
    for<'a> fn(&mut ConfiguredFeaturePlaceContext<'a, L>, &ConfiguredFeatureKind) -> bool;

impl FeatureDecorationRunner {
    pub(super) fn place_configured_feature(
        region: &impl WorldGenLevel,
        registry: &Registry,
        random: &mut WorldgenRandom,
        feature: &ConfiguredFeatureRef,
        origin: BlockPos,
        biome_zoom_seed: i64,
    ) -> bool {
        let kind = Self::configured_feature_kind(feature);
        Self::place_configured_feature_kind(region, registry, random, kind, origin, biome_zoom_seed)
    }

    pub(crate) fn place_configured_feature_kind(
        region: &impl WorldGenLevel,
        registry: &Registry,
        random: &mut WorldgenRandom,
        kind: &ConfiguredFeatureKind,
        origin: BlockPos,
        biome_zoom_seed: i64,
    ) -> bool {
        if !region.can_write_to_chunk(
            SectionPos::block_to_section_coord(origin.x()),
            SectionPos::block_to_section_coord(origin.z()),
        ) {
            return false;
        }

        let placer = Self::configured_feature_placer(kind);
        let mut context = ConfiguredFeaturePlaceContext {
            region,
            registry,
            random,
            origin,
            biome_zoom_seed,
        };
        placer(&mut context, kind)
    }

    pub(super) fn configured_feature_kind(
        feature: &ConfiguredFeatureRef,
    ) -> &ConfiguredFeatureKind {
        match feature {
            ConfiguredFeatureRef::Reference(configured_feature) => &configured_feature.kind,
            ConfiguredFeatureRef::Inline(configured_feature) => configured_feature,
        }
    }

    fn configured_feature_placer<L: WorldGenLevel>(
        kind: &ConfiguredFeatureKind,
    ) -> ConfiguredFeaturePlacer<L> {
        match kind {
            ConfiguredFeatureKind::Bamboo(_) => place_bamboo,
            ConfiguredFeatureKind::BasaltColumns(_) => place_basalt_columns,
            ConfiguredFeatureKind::BasaltPillar => place_basalt_pillar,
            ConfiguredFeatureKind::BlockBlob(_) => place_block_blob,
            ConfiguredFeatureKind::BlockColumn(_) => place_block_column,
            ConfiguredFeatureKind::BlockPile(_) => place_block_pile,
            ConfiguredFeatureKind::BlueIce => place_blue_ice,
            ConfiguredFeatureKind::BonusChest => place_bonus_chest,
            ConfiguredFeatureKind::ChorusPlant => place_chorus_plant,
            ConfiguredFeatureKind::CoralClaw => place_coral_claw,
            ConfiguredFeatureKind::CoralMushroom => place_coral_mushroom,
            ConfiguredFeatureKind::CoralTree => place_coral_tree,
            ConfiguredFeatureKind::DeltaFeature(_) => place_delta_feature,
            ConfiguredFeatureKind::DesertWell => place_desert_well,
            ConfiguredFeatureKind::Disk(_) => place_disk,
            ConfiguredFeatureKind::DripstoneCluster(_) => place_dripstone_cluster,
            ConfiguredFeatureKind::EndGateway(_) => place_end_gateway,
            ConfiguredFeatureKind::EndIsland => place_end_island,
            ConfiguredFeatureKind::EndPlatform => place_end_platform,
            ConfiguredFeatureKind::EndSpike(_) => place_end_spike,
            ConfiguredFeatureKind::FallenTree(_) => place_fallen_tree,
            ConfiguredFeatureKind::Fossil(_) => place_fossil,
            ConfiguredFeatureKind::FillLayer(_) => place_fill_layer,
            ConfiguredFeatureKind::FreezeTopLayer => place_freeze_top_layer,
            ConfiguredFeatureKind::Geode(_) => place_geode,
            ConfiguredFeatureKind::GlowstoneBlob => place_glowstone_blob,
            ConfiguredFeatureKind::HugeBrownMushroom(_) => place_huge_brown_mushroom,
            ConfiguredFeatureKind::HugeFungus(_) => place_huge_fungus,
            ConfiguredFeatureKind::HugeRedMushroom(_) => place_huge_red_mushroom,
            ConfiguredFeatureKind::Iceberg(_) => place_iceberg,
            ConfiguredFeatureKind::Kelp => place_kelp,
            ConfiguredFeatureKind::Lake(_) => place_lake,
            ConfiguredFeatureKind::LargeDripstone(_) => place_large_dripstone,
            ConfiguredFeatureKind::MonsterRoom => place_monster_room,
            ConfiguredFeatureKind::MultifaceGrowth(_) => place_multiface_growth,
            ConfiguredFeatureKind::NetherForestVegetation(_) => place_nether_forest_vegetation,
            ConfiguredFeatureKind::NetherrackReplaceBlobs(_) => place_netherrack_replace_blobs,
            ConfiguredFeatureKind::Ore(_) => place_ore,
            ConfiguredFeatureKind::PointedDripstone(_) => place_pointed_dripstone,
            ConfiguredFeatureKind::RandomBooleanSelector(_) => place_random_boolean_selector,
            ConfiguredFeatureKind::RandomSelector(_) => place_random_selector,
            ConfiguredFeatureKind::WeightedRandomSelector(_) => place_weighted_random_selector,
            ConfiguredFeatureKind::RootSystem(_) => place_root_system,
            ConfiguredFeatureKind::ScatteredOre(_) => place_scattered_ore,
            ConfiguredFeatureKind::SculkPatch(_) => place_sculk_patch,
            ConfiguredFeatureKind::SeaPickle(_) => place_sea_pickle,
            ConfiguredFeatureKind::Seagrass(_) => place_seagrass,
            ConfiguredFeatureKind::Sequence(_) => place_sequence,
            ConfiguredFeatureKind::SimpleBlock(_) => place_simple_block,
            ConfiguredFeatureKind::SimpleRandomSelector(_) => place_simple_random_selector,
            ConfiguredFeatureKind::Speleothem(_) => place_speleothem,
            ConfiguredFeatureKind::SpeleothemCluster(_) => place_speleothem_cluster,
            ConfiguredFeatureKind::Spike(_) => place_spike,
            ConfiguredFeatureKind::SpringFeature(_) => place_spring_feature,
            ConfiguredFeatureKind::Template(_) => place_template,
            ConfiguredFeatureKind::Tree(_) => place_tree,
            ConfiguredFeatureKind::TwistingVines(_) => place_twisting_vines,
            ConfiguredFeatureKind::UnderwaterMagma(_) => place_underwater_magma,
            ConfiguredFeatureKind::VegetationPatch(_) => place_vegetation_patch,
            ConfiguredFeatureKind::Vines => place_vines,
            ConfiguredFeatureKind::VoidStartPlatform => place_void_start_platform,
            ConfiguredFeatureKind::WaterloggedVegetationPatch(_) => {
                place_waterlogged_vegetation_patch
            }
            ConfiguredFeatureKind::WeepingVines => place_weeping_vines,
        }
    }
}

fn place_random_boolean_selector<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::RandomBooleanSelector(config) = kind else {
        panic!("random_boolean_selector placer received wrong configured feature kind");
    };
    let selected_feature = if context.random.next_bool() {
        &config.feature_true
    } else {
        &config.feature_false
    };
    FeatureDecorationRunner::place_placed_feature_ref(
        context.region,
        context.registry,
        context.random,
        context.origin,
        selected_feature,
        context.biome_zoom_seed,
    )
}

fn place_random_selector<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::RandomSelector(config) = kind else {
        panic!("random_selector placer received wrong configured feature kind");
    };
    for weighted_feature in &config.features {
        let roll = context.random.next_f32();
        if roll < weighted_feature.chance {
            return FeatureDecorationRunner::place_placed_feature_ref(
                context.region,
                context.registry,
                context.random,
                context.origin,
                &weighted_feature.feature,
                context.biome_zoom_seed,
            );
        }
    }

    FeatureDecorationRunner::place_placed_feature_ref(
        context.region,
        context.registry,
        context.random,
        context.origin,
        &config.default,
        context.biome_zoom_seed,
    )
}

fn place_weighted_random_selector<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::WeightedRandomSelector(config) = kind else {
        panic!("weighted_random_selector placer received wrong configured feature kind");
    };
    let Some(feature_index) = weighted_index(
        config.features.len(),
        |index| config.features[index].weight,
        context.random,
    ) else {
        return false;
    };
    FeatureDecorationRunner::place_placed_feature_ref(
        context.region,
        context.registry,
        context.random,
        context.origin,
        &config.features[feature_index].data,
        context.biome_zoom_seed,
    )
}

fn place_simple_random_selector<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::SimpleRandomSelector(config) = kind else {
        panic!("simple_random_selector placer received wrong configured feature kind");
    };
    assert!(
        !config.features.is_empty(),
        "simple random selector feature list must not be empty"
    );
    let Ok(feature_count) = i32::try_from(config.features.len()) else {
        panic!(
            "simple random selector feature count {} exceeds i32 range",
            config.features.len()
        );
    };
    let feature_index = context.random.next_i32_bounded(feature_count) as usize;
    FeatureDecorationRunner::place_placed_feature_ref(
        context.region,
        context.registry,
        context.random,
        context.origin,
        &config.features[feature_index],
        context.biome_zoom_seed,
    )
}

fn place_sequence<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::Sequence(config) = kind else {
        panic!("sequence placer received wrong configured feature kind");
    };
    for feature in &config.features {
        if !FeatureDecorationRunner::place_placed_feature_ref(
            context.region,
            context.registry,
            context.random,
            context.origin,
            feature,
            context.biome_zoom_seed,
        ) {
            return false;
        }
    }
    true
}

fn place_template<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::Template(config) = kind else {
        panic!("template placer received wrong configured feature kind");
    };
    let Some(template_index) = weighted_index(
        config.templates.len(),
        |index| config.templates[index].weight,
        context.random,
    ) else {
        panic!("template feature has no selectable templates");
    };
    let entry = &config.templates[template_index].data;
    let Ok(rotation_count) = i32::try_from(entry.rotations.len()) else {
        panic!(
            "template feature rotation count {} exceeds i32 range",
            entry.rotations.len()
        );
    };
    assert!(
        rotation_count != 0,
        "template feature entry {} has no rotations",
        entry.id
    );
    let rotation = entry.rotations[context.random.next_i32_bounded(rotation_count) as usize];
    let template = match StructureTemplate::load_vanilla(context.registry, &entry.id) {
        Ok(template) => template,
        Err(err) => panic!("{err}"),
    };
    let size = template.size(Rotation::None);
    let position = template_feature_position(context.origin, rotation, size);
    let settings = StructurePlaceSettings {
        mirror: StructureMirror::None,
        rotation,
        rotation_pivot: BlockPos::ZERO,
        bounding_box: template_feature_bounding_box(context.region),
        processors: &[],
        block_ignore: StructureBlockIgnore::None,
        late_block_ignore: StructureBlockIgnore::None,
        replace_jigsaws: false,
        projection: None,
        processor_random: StructureProcessorRandom::Placement,
        liquid_settings: LiquidSettingsData::ApplyWaterlogging,
        ignore_entities: false,
    };

    template.place_in_world(
        context.region,
        context.registry,
        position,
        position,
        &settings,
        context.random,
        UpdateFlags::UPDATE_ALL,
    )
}

fn weighted_index(
    len: usize,
    weight_at: impl Fn(usize) -> u32,
    random: &mut WorldgenRandom,
) -> Option<usize> {
    let total_weight = (0..len)
        .map(|index| u64::from(weight_at(index)))
        .sum::<u64>();
    if total_weight == 0 {
        return None;
    }
    let Ok(total_weight_i32) = i32::try_from(total_weight) else {
        panic!("weighted feature total weight {total_weight} exceeds i32 range");
    };
    let mut selection = random.next_i32_bounded(total_weight_i32) as u64;
    for index in 0..len {
        let weight = u64::from(weight_at(index));
        if selection < weight {
            return Some(index);
        }
        selection -= weight;
    }
    None
}

const fn template_feature_position(origin: BlockPos, rotation: Rotation, size: IVec3) -> BlockPos {
    let west_offset = rotation.rotate(Direction::West).offset();
    let north_offset = rotation.rotate(Direction::North).offset();
    origin.offset(
        west_offset.0 * (size.x / 2) + north_offset.0 * (size.z / 2),
        0,
        west_offset.2 * (size.x / 2) + north_offset.2 * (size.z / 2),
    )
}

fn template_feature_bounding_box(region: &impl WorldGenLevel) -> BoundingBox {
    BoundingBox::new(
        IVec3::new(i32::MIN, region.min_y(), i32::MIN),
        IVec3::new(i32::MAX, region.max_y_exclusive() - 1, i32::MAX),
    )
}

fn place_bamboo<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::Bamboo(config) = kind else {
        panic!("bamboo placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_bamboo_feature(
        context.region,
        context.random,
        config,
        context.origin,
    )
}

fn place_simple_block<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::SimpleBlock(config) = kind else {
        panic!("simple_block placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_simple_block_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
    )
}

fn place_block_blob<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::BlockBlob(config) = kind else {
        panic!("block_blob placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_block_blob_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
    )
}

fn place_vegetation_patch<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::VegetationPatch(config) = kind else {
        panic!("vegetation_patch placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_vegetation_patch_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
        context.biome_zoom_seed,
    )
}

fn place_waterlogged_vegetation_patch<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::WaterloggedVegetationPatch(config) = kind else {
        panic!("waterlogged_vegetation_patch placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_waterlogged_vegetation_patch_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
        context.biome_zoom_seed,
    )
}

fn place_block_column<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::BlockColumn(config) = kind else {
        panic!("block_column placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_block_column_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
    )
}

fn place_block_pile<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::BlockPile(config) = kind else {
        panic!("block_pile placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_block_pile_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
    )
}

fn place_disk<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::Disk(config) = kind else {
        panic!("disk placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_disk_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
    )
}

fn place_basalt_pillar<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::BasaltPillar = kind else {
        panic!("basalt_pillar placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_basalt_pillar_feature(
        context.region,
        context.random,
        context.origin,
    )
}

fn place_basalt_columns<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::BasaltColumns(config) = kind else {
        panic!("basalt_columns placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_basalt_columns_feature(
        context.region,
        context.random,
        config,
        context.origin,
    )
}

fn place_blue_ice<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::BlueIce = kind else {
        panic!("blue_ice placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_blue_ice_feature(context.region, context.random, context.origin)
}

fn place_bonus_chest<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::BonusChest = kind else {
        panic!("bonus_chest placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_bonus_chest_feature(
        context.region,
        context.random,
        context.origin,
    )
}

fn place_chorus_plant<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::ChorusPlant = kind else {
        panic!("chorus_plant placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_chorus_plant_feature(
        context.region,
        context.random,
        context.origin,
    )
}

fn place_coral_claw<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::CoralClaw = kind else {
        panic!("coral_claw placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_coral_claw_feature(
        context.region,
        context.registry,
        context.random,
        context.origin,
    )
}

fn place_coral_mushroom<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::CoralMushroom = kind else {
        panic!("coral_mushroom placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_coral_mushroom_feature(
        context.region,
        context.registry,
        context.random,
        context.origin,
    )
}

fn place_coral_tree<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::CoralTree = kind else {
        panic!("coral_tree placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_coral_tree_feature(
        context.region,
        context.registry,
        context.random,
        context.origin,
    )
}

fn place_delta_feature<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::DeltaFeature(config) = kind else {
        panic!("delta_feature placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_delta_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
    )
}

fn place_desert_well<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::DesertWell = kind else {
        panic!("desert_well placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_desert_well_feature(
        context.region,
        context.random,
        context.origin,
    )
}

fn place_end_gateway<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::EndGateway(config) = kind else {
        panic!("end_gateway placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_end_gateway_feature(context.region, config, context.origin)
}

fn place_end_island<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::EndIsland = kind else {
        panic!("end_island placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_end_island_feature(
        context.region,
        context.random,
        context.origin,
    )
}

fn place_end_platform<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::EndPlatform = kind else {
        panic!("end_platform placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_end_platform_feature(context.region, context.origin)
}

fn place_end_spike<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::EndSpike(config) = kind else {
        panic!("end_spike placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_end_spike_feature(
        context.region,
        context.random,
        config,
        context.origin,
    )
}

fn place_geode<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::Geode(config) = kind else {
        panic!("geode placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_geode_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
    )
}

fn place_glowstone_blob<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::GlowstoneBlob = kind else {
        panic!("glowstone_blob placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_glowstone_blob_feature(
        context.region,
        context.random,
        context.origin,
    )
}

fn place_huge_brown_mushroom<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::HugeBrownMushroom(config) = kind else {
        panic!("huge_brown_mushroom placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_huge_brown_mushroom_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
    )
}

fn place_huge_red_mushroom<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::HugeRedMushroom(config) = kind else {
        panic!("huge_red_mushroom placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_huge_red_mushroom_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
    )
}

fn place_huge_fungus<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::HugeFungus(config) = kind else {
        panic!("huge_fungus placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_huge_fungus_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
    )
}

fn place_iceberg<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::Iceberg(config) = kind else {
        panic!("iceberg placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_iceberg_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
    )
}

fn place_netherrack_replace_blobs<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::NetherrackReplaceBlobs(config) = kind else {
        panic!("netherrack_replace_blobs placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_netherrack_replace_blobs_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
    )
}

fn place_nether_forest_vegetation<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::NetherForestVegetation(config) = kind else {
        panic!("nether_forest_vegetation placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_nether_forest_vegetation_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
    )
}

fn place_twisting_vines<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::TwistingVines(config) = kind else {
        panic!("twisting_vines placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_twisting_vines_feature(
        context.region,
        context.random,
        config,
        context.origin,
    )
}

fn place_vines<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::Vines = kind else {
        panic!("vines placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_vines_feature(context.region, context.origin)
}

fn place_void_start_platform<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::VoidStartPlatform = kind else {
        panic!("void_start_platform placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_void_start_platform_feature(context.region, context.origin)
}

fn place_weeping_vines<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::WeepingVines = kind else {
        panic!("weeping_vines placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_weeping_vines_feature(
        context.region,
        context.random,
        context.origin,
    )
}

fn place_spring_feature<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::SpringFeature(config) = kind else {
        panic!("spring_feature placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_spring_feature(
        context.region,
        context.registry,
        config,
        context.origin,
    )
}

fn place_kelp<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::Kelp = kind else {
        panic!("kelp placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_kelp_feature(context.region, context.random, context.origin)
}

fn place_lake<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::Lake(config) = kind else {
        panic!("lake placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_lake_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
        context.biome_zoom_seed,
    )
}

fn place_monster_room<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::MonsterRoom = kind else {
        panic!("monster_room placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_monster_room_feature(
        context.region,
        context.random,
        context.origin,
    )
}

fn place_fill_layer<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::FillLayer(config) = kind else {
        panic!("fill_layer placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_fill_layer_feature(context.region, context.origin, *config)
}

fn place_freeze_top_layer<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::FreezeTopLayer = kind else {
        panic!("freeze_top_layer placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_freeze_top_layer_feature(
        context.region,
        context.registry,
        context.origin,
        context.biome_zoom_seed,
    )
}

fn place_multiface_growth<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::MultifaceGrowth(config) = kind else {
        panic!("multiface_growth placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_multiface_growth_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
    )
}

fn place_sea_pickle<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::SeaPickle(config) = kind else {
        panic!("sea_pickle placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_sea_pickle_feature(
        context.region,
        context.random,
        config,
        context.origin,
    )
}

fn place_seagrass<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::Seagrass(config) = kind else {
        panic!("seagrass placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_seagrass_feature(
        context.region,
        context.random,
        config,
        context.origin,
    )
}

fn place_underwater_magma<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::UnderwaterMagma(config) = kind else {
        panic!("underwater_magma placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_underwater_magma_feature(
        context.region,
        context.random,
        config,
        context.origin,
    )
}

fn place_pointed_dripstone<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::PointedDripstone(config) = kind else {
        panic!("pointed_dripstone placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_pointed_dripstone_feature(
        context.region,
        context.random,
        config,
        context.origin,
    )
}

fn place_dripstone_cluster<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::DripstoneCluster(config) = kind else {
        panic!("dripstone_cluster placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_dripstone_cluster_feature(
        context.region,
        context.random,
        config,
        context.origin,
    )
}

fn place_speleothem<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::Speleothem(config) = kind else {
        panic!("speleothem placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_speleothem_feature(
        context.region,
        context.random,
        config,
        context.origin,
    )
}

fn place_speleothem_cluster<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::SpeleothemCluster(config) = kind else {
        panic!("speleothem_cluster placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_speleothem_cluster_feature(
        context.region,
        context.random,
        config,
        context.origin,
    )
}

fn place_large_dripstone<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::LargeDripstone(config) = kind else {
        panic!("large_dripstone placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_large_dripstone_feature(
        context.region,
        context.random,
        config,
        context.origin,
    )
}

fn place_spike<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::Spike(config) = kind else {
        panic!("spike placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_spike_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
    )
}

fn place_ore<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::Ore(config) = kind else {
        panic!("ore placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_ore_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
    )
}

fn place_scattered_ore<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::ScatteredOre(config) = kind else {
        panic!("scattered_ore placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_scattered_ore_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
    )
}

fn place_sculk_patch<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::SculkPatch(config) = kind else {
        panic!("sculk_patch placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_sculk_patch_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
    )
}

fn place_tree<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::Tree(config) = kind else {
        panic!("tree placer received wrong configured feature kind");
    };

    // The tree itself is written against `LevelAccessor` so a sapling can grow
    // one in a live world. The pale moss patch under a pale oak is a nested
    // configured feature, and this dispatcher is the only thing that can place
    // one, so the tree reports where it wanted them and they are placed here.
    let mut ground_features = Vec::new();
    let placed = FeatureDecorationRunner::place_tree_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
        &mut ground_features,
    );

    if placed {
        place_tree_ground_features(context, &ground_features);
    }

    placed
}

/// Places the moss patches a tree's decorators asked for.
///
/// The tree code runs over any `LevelAccessor` so a sapling can grow one, and a
/// nested configured feature can only be placed from this dispatcher, so the
/// decorators hand back where they wanted one and this puts it there.
fn place_tree_ground_features<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    origins: &[BlockPos],
) {
    if origins.is_empty() {
        return;
    }

    let pale_moss_patch_key = Identifier::vanilla_static("pale_moss_patch");
    let Some(pale_moss_patch) = context
        .registry
        .configured_features
        .by_key(&pale_moss_patch_key)
    else {
        panic!(
            "pale moss tree decorator references unknown configured feature {pale_moss_patch_key}"
        );
    };
    let kind = pale_moss_patch.kind.clone();

    for origin in origins {
        FeatureDecorationRunner::place_configured_feature_kind(
            context.region,
            context.registry,
            context.random,
            &kind,
            *origin,
            context.biome_zoom_seed,
        );
    }
}

fn place_fallen_tree<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::FallenTree(config) = kind else {
        panic!("fallen_tree placer received wrong configured feature kind");
    };
    // A fallen log carries stump and log decorators, which can ask for a
    // ground feature exactly as a standing tree's can; see `place_tree`.
    let mut ground_features = Vec::new();
    let placed = FeatureDecorationRunner::place_fallen_tree_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
        &mut ground_features,
    );
    place_tree_ground_features(context, &ground_features);
    placed
}

fn place_fossil<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::Fossil(config) = kind else {
        panic!("fossil placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_fossil_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
    )
}

fn place_root_system<L: WorldGenLevel>(
    context: &mut ConfiguredFeaturePlaceContext<'_, L>,
    kind: &ConfiguredFeatureKind,
) -> bool {
    let ConfiguredFeatureKind::RootSystem(config) = kind else {
        panic!("root_system placer received wrong configured feature kind");
    };
    FeatureDecorationRunner::place_root_system_feature(
        context.region,
        context.registry,
        context.random,
        config,
        context.origin,
        context.biome_zoom_seed,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_feature_position_uses_template_depth_for_north_offset() {
        assert_eq!(
            template_feature_position(
                BlockPos::new(100, 64, 200),
                Rotation::None,
                IVec3::new(10, 30, 6),
            ),
            BlockPos::new(95, 64, 197)
        );
    }
}
