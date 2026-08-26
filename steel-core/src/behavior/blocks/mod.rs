//! Block behavior implementations for vanilla blocks.
//!
//! The actual behavior registration is auto-generated from classes.json.
//! See `src/generated/behaviors.rs` for the generated registration code.

pub(crate) mod building;
mod colored;
mod container;
mod decoration;
mod eggs;
mod falling;
mod fluid;
mod portal;
mod redstone;
mod sculk;
mod spawner;
mod utils;
pub mod vegetation;

pub use building::{
    AmethystBlock, AmethystClusterBlock, BarrierBlock, BedBlock, BrushableBlock,
    BuddingAmethystBlock, CampfireBlock, CauldronBlock, ComposterBlock, ConduitBlock, DoorBlock,
    DropExperienceBlock, FenceBlock, FenceGateBlock, FrostedIceBlock, GlazedTerracottaBlock,
    HayBlock, HeavyCoreBlock, HoneyBlock, IceBlock, Infested, InfestedBlock,
    InfestedRotatedPillarBlock, IronBarsBlock, LadderBlock, LavaCauldronBlock,
    LayeredCauldronBlock, LightBlock, MagmaBlock, MudBlock, PotentSulfurBlock, PowderSnowBlock,
    RotatedPillarBlock, ScaffoldingBlock, SlabBlock, SlimeBlock, SnowLayerBlock, SoulSandBlock,
    SpongeBlock, StairBlock, TrapDoorBlock, WallBlock, WaterloggedTransparentBlock, WeatherState,
    WeatheringCopper, WeatheringCopperBarsBlock, WeatheringCopperDoorBlock,
    WeatheringCopperFullBlock, WeatheringCopperGrateBlock, WeatheringCopperSlabBlock,
    WeatheringCopperStairBlock, WeatheringCopperTrapDoorBlock, WebBlock, WetSpongeBlock,
    host_state_by_infested, infested_state_by_host, is_compatible_host_block, spawn_infestation,
};
pub use colored::StainedGlassPaneBlock;
pub use container::{
    AnvilBlock, BarrelBlock, BeaconBlock, BeehiveBlock, BlastFurnaceBlock, BrewingStandBlock,
    CartographyTableBlock, ChestBlock, ChiseledBookShelfBlock, CopperChest, CopperChestBlock,
    CrafterBlock, CraftingTableBlock, DispenserBlock, DropperBlock, EnchantingTableBlock,
    EnderChestBlock, FurnaceBlock, GrindstoneBlock, HopperBlock, JukeboxBlock, LecternBlock,
    LoomBlock, ShelfBlock, ShulkerBoxBlock, SmithingTableBlock, SmokerBlock, StonecutterBlock,
    TrappedChestBlock, WeatheringCopperChestBlock, count_enchanting_power,
    signal_lectern_page_change, take_book_from,
};
pub use decoration::{
    BannerBlock, BellBlock, CakeBlock, CandleBlock, CandleCakeBlock, CeilingHangingSignBlock,
    ChainBlock, CopperGolemStatueBlock, DecoratedPotBlock, DriedGhastBlock, EndRodBlock,
    FlowerPotBlock, LanternBlock, PiglinWallSkullBlock, PlayerHeadBlock, PlayerWallHeadBlock,
    SkullBlock, StandingSignBlock, TorchBlock, WallBannerBlock, WallHangingSignBlock,
    WallSignBlock, WallSkullBlock, WallTorchBlock, WeatheringCopperChainBlock,
    WeatheringCopperGolemStatueBlock, WeatheringLanternBlock, WitherSkullBlock,
    WitherWallSkullBlock,
};
pub use eggs::{FrogspawnBlock, SnifferEggBlock, TurtleEggBlock};
pub use falling::{ConcretePowderBlock, DragonEggBlock, FallingBlock, SandBlock};
pub use fluid::{BubbleColumnBlock, LiquidBlock};
pub use portal::{
    EndGatewayBlock, EndPortalBlock, EndPortalFrameBlock, FireBlock, NetherPortalBlock,
    RespawnAnchorBlock, SoulFireBlock,
};
pub use redstone::{
    ButtonBlock, ComparatorBlock, CopperBulbBlock, DaylightDetectorBlock, DetectorRailBlock,
    LeverBlock, LightningRod, LightningRodBlock, MovingPistonBlock, NoteBlock, ObserverBlock,
    PistonBaseBlock, PistonHeadBlock, PoweredBlock, PoweredRailBlock, PressurePlateBlock,
    PressurePlateSensitivity, RailBlock, RedStoneOreBlock, RedStoneWireBlock, RedstoneLampBlock,
    RedstoneTorchBlock, RedstoneWallTorchBlock, RepeaterBlock, TargetBlock, TntBlock,
    TripWireBlock, TripWireHookBlock, WeatheringCopperBulbBlock, WeatheringLightningRodBlock,
    WeightedPressurePlateBlock, rail_shape_at,
};
pub(crate) use redstone::{MAX_REDSTONE_SIGNAL, MIN_REDSTONE_SIGNAL};
pub use sculk::behavior_of as sculk_behavior_of;
pub use sculk::{
    CalibratedSculkSensorBlock, ChargeCursor, SculkBehaviorKind, SculkBlock, SculkCatalystBlock,
    SculkSensorBlock, SculkShriekerBlock, SculkSpreader, can_activate_sculk_sensor,
    deactivate_sculk_sensor, sculk_sensor_phase, try_resonate_vibration,
};
pub use spawner::{SpawnerBlock, TrialSpawnerBlock, VaultBlock};
pub(crate) use utils::multiface_face_property;
pub use vegetation::{
    AttachedStemBlock, AzaleaBlock, BambooSaplingBlock, BambooStalkBlock, BeetrootBlock,
    CactusBlock, CactusFlowerBlock, CarrotBlock, CarvedPumpkinBlock, CocoaBlock, CoralBlock,
    CropBlock, DoublePlantBlock, FlowerBlock, GrassBlock, MangroveLeavesBlock, MangroveRootsBlock,
    MultifaceBlock, MyceliumBlock, NetherSproutsBlock, NetherWartBlock, NetherrackBlock,
    NyliumBlock, PitcherCropBlock, PotatoBlock, PumpkinBlock, RootedDirtBlock, SeagrassBlock,
    SnowyBlock, StemBlock, SugarCaneBlock, SweetBerryBushBlock, TallFlowerBlock, TallGrassBlock,
    TallSeagrassBlock, TintedParticleLeavesBlock, TorchflowerCropBlock,
    UntintedParticleLeavesBlock,
};
pub use vegetation::{
    BaseCoralFanBlock, BaseCoralPlantBlock, BaseCoralWallFanBlock, BigDripleafBlock,
    BigDripleafStemBlock, BushBlock, CarpetBlock, CaveVinesBlock, CaveVinesPlantBlock,
    ChorusFlowerBlock, ChorusPlantBlock, CoralFanBlock, CoralPlantBlock, CoralWallFanBlock,
    CreakingHeartBlock, DirtPathBlock, DryVegetationBlock, EyeblossomBlock, EyeblossomType,
    FarmlandBlock, FireflyBushBlock, FlowerBedBlock, GlowLichenBlock, HangingMossBlock,
    HangingRootsBlock, HugeMushroomBlock, KelpBlock, KelpPlantBlock, LeafLitterBlock, LilyPadBlock,
    MangrovePropaguleBlock, MossyCarpetBlock, MushroomBlock, NetherFungusBlock, NetherRootsBlock,
    PointedDripstoneBlock, SaplingBlock, SculkVeinBlock, SeaPickleBlock, ShortDryGrassBlock,
    SmallDripleafBlock, SporeBlossomBlock, SulfurSpikeBlock, TallDryGrassBlock, TwistingVinesBlock,
    TwistingVinesPlantBlock, VineBlock, WeepingVinesBlock, WeepingVinesPlantBlock, WitherRoseBlock,
    WoolCarpetBlock,
};
pub(crate) use vegetation::{CREAKING_HEART_STATE, creaking_heart_awake_or_dormant};
