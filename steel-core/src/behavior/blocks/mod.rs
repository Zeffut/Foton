//! Block behavior implementations for vanilla blocks.
//!
//! The actual behavior registration is auto-generated from classes.json.
//! See `src/generated/behaviors.rs` for the generated registration code.

mod building;
mod colored;
mod container;
mod decoration;
mod falling;
mod fluid;
mod portal;
mod redstone;
mod utils;
pub mod vegetation;

pub use building::{
    AmethystBlock, AmethystClusterBlock, BarrierBlock, BedBlock, BrushableBlock,
    BuddingAmethystBlock, CampfireBlock, CauldronBlock, ComposterBlock, ConduitBlock, DoorBlock,
    DropExperienceBlock, FenceBlock, FenceGateBlock, GlazedTerracottaBlock, HayBlock,
    HeavyCoreBlock, HoneyBlock, IceBlock, Infested, InfestedBlock, InfestedRotatedPillarBlock,
    IronBarsBlock, LadderBlock, LavaCauldronBlock, LayeredCauldronBlock, MagmaBlock, MudBlock,
    PotentSulfurBlock, PowderSnowBlock, RotatedPillarBlock, ScaffoldingBlock, SlabBlock,
    SlimeBlock, SnowLayerBlock, SpongeBlock, StairBlock, TrapDoorBlock, WallBlock,
    WaterloggedTransparentBlock, WeatherState, WeatheringCopper, WeatheringCopperBarsBlock,
    WeatheringCopperDoorBlock, WeatheringCopperFullBlock, WeatheringCopperGrateBlock,
    WeatheringCopperSlabBlock, WeatheringCopperStairBlock, WeatheringCopperTrapDoorBlock, WebBlock,
    WetSpongeBlock, host_state_by_infested, infested_state_by_host, is_compatible_host_block,
    spawn_infestation,
};
pub use colored::StainedGlassPaneBlock;
pub use container::{
    AnvilBlock, BarrelBlock, BeaconBlock, BeehiveBlock, BlastFurnaceBlock, BrewingStandBlock,
    ChestBlock, ChiseledBookShelfBlock, CrafterBlock, CraftingTableBlock, DispenserBlock,
    DropperBlock, EnchantingTableBlock, EnderChestBlock, FurnaceBlock, GrindstoneBlock,
    HopperBlock, JukeboxBlock, LecternBlock, LoomBlock, ShulkerBoxBlock, SmithingTableBlock,
    SmokerBlock, StonecutterBlock, TrappedChestBlock, count_enchanting_power,
    signal_lectern_page_change, take_book_from,
};
pub use decoration::{
    BannerBlock, BellBlock, CakeBlock, CandleBlock, CandleCakeBlock, CeilingHangingSignBlock,
    ChainBlock, DecoratedPotBlock, EndRodBlock, FlowerPotBlock, LanternBlock, StandingSignBlock,
    TorchBlock, WallBannerBlock, WallHangingSignBlock, WallSignBlock, WallTorchBlock,
    WeatheringCopperChainBlock, WeatheringLanternBlock,
};
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
pub(crate) use utils::multiface_face_property;
pub use vegetation::{
    AttachedStemBlock, AzaleaBlock, BambooSaplingBlock, BambooStalkBlock, BeetrootBlock,
    CactusBlock, CactusFlowerBlock, CarrotBlock, CocoaBlock, CoralBlock, CropBlock,
    DoublePlantBlock, FlowerBlock, GrassBlock, MangroveLeavesBlock, MultifaceBlock, MyceliumBlock,
    NetherSproutsBlock, NetherWartBlock, PitcherCropBlock, PotatoBlock, PumpkinBlock,
    RootedDirtBlock, SeagrassBlock, SnowyBlock, StemBlock, SugarCaneBlock, SweetBerryBushBlock,
    TallFlowerBlock, TallGrassBlock, TallSeagrassBlock, TintedParticleLeavesBlock,
    TorchflowerCropBlock, UntintedParticleLeavesBlock,
};
pub use vegetation::{
    BaseCoralFanBlock, BaseCoralPlantBlock, BaseCoralWallFanBlock, BigDripleafBlock,
    BigDripleafStemBlock, BushBlock, CarpetBlock, CaveVinesBlock, CaveVinesPlantBlock,
    ChorusFlowerBlock, ChorusPlantBlock, CoralFanBlock, CoralPlantBlock, CoralWallFanBlock,
    DirtPathBlock, DryVegetationBlock, EyeblossomBlock, EyeblossomType, FarmlandBlock,
    FireflyBushBlock, FlowerBedBlock, GlowLichenBlock, HangingMossBlock, HangingRootsBlock,
    HugeMushroomBlock, KelpBlock, KelpPlantBlock, LeafLitterBlock, LilyPadBlock,
    MangrovePropaguleBlock, MossyCarpetBlock, MushroomBlock, NetherFungusBlock, NetherRootsBlock,
    PointedDripstoneBlock, SaplingBlock, SculkVeinBlock, SeaPickleBlock, ShortDryGrassBlock,
    SmallDripleafBlock, SporeBlossomBlock, SulfurSpikeBlock, TallDryGrassBlock, TwistingVinesBlock,
    TwistingVinesPlantBlock, VineBlock, WeepingVinesBlock, WeepingVinesPlantBlock, WitherRoseBlock,
    WoolCarpetBlock,
};
