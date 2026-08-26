//! Concrete entity implementations.

pub mod decoration;
pub mod mobs;
pub mod objects;
mod raw;

pub use decoration::ArmorStandEntity;
pub use mobs::ambient::BatEntity;
pub use mobs::bosses::WitherBoss;
pub use mobs::hostile::{
    BlazeEntity, CaveSpiderEntity, CreeperEntity, DrownedEntity, ElderGuardianEntity,
    EndermanEntity, EndermiteEntity, EvokerEntity, GhastEntity, GuardianEntity, HoglinEntity,
    HuskEntity, IllusionerEntity, MagmaCubeEntity, PhantomEntity, PiglinBruteEntity, PiglinEntity,
    PillagerEntity, RavagerEntity, ShulkerEntity, SilverfishEntity, SkeletonEntity, SlimeEntity,
    SpiderEntity, StrayEntity, VexEntity, VindicatorEntity, WitchEntity, WitherSkeletonEntity,
    ZoglinEntity, ZombieEntity, ZombifiedPiglinEntity,
};
pub use mobs::neutral::{CopperGolemEntity, IronGolemEntity, SnowGolemEntity, WolfEntity};
pub use mobs::npc::{VillagerEntity, WanderingTraderEntity, ZombieVillagerEntity};
pub use mobs::passive::{
    BeeEntity, CatEntity, ChickenEntity, CowEntity, DonkeyEntity, FoxEntity, FoxVariant,
    FrogEntity, GoatEntity, HorseEntity, HorseMarkings, HorseVariant, LlamaEntity, MuleEntity,
    MushroomCowEntity, OcelotEntity, ParrotEntity, ParrotVariant, PigEntity, PolarBearEntity,
    RabbitEntity, RabbitVariant, SheepEntity, SkeletonHorseEntity, StriderEntity,
    TraderLlamaEntity, TurtleEntity, ZombieHorseEntity,
};
pub use mobs::water::{
    CodEntity, DolphinEntity, GlowSquidEntity, MAX_TADPOLES_SPAWN_EXCLUSIVE, MIN_TADPOLES_SPAWN,
    PufferfishEntity, SalmonEntity, SquidEntity, TICKS_TO_BE_FROG, TadpoleEntity,
    TropicalFishEntity, TropicalFishPattern, TropicalFishVariant, spawn_tadpoles_from_frogspawn,
};
pub use objects::display_ui::{
    BlockDisplayEntity, InteractionEntity, ItemDisplayEntity, ItemFrameEntity,
    LeashFenceKnotEntity, PaintingEntity, TextDisplayEntity,
};
pub use objects::explosives::{EndCrystalEntity, PrimedTntEntity};
pub use objects::items::{ExperienceOrbEntity, FallingBlockEntity, ItemEntity};
pub use objects::projectiles::{
    ArrowEntity, DragonFireballEntity, EnderPearlEntity, EvokerFangsEntity, ExperienceBottleEntity,
    FireworkRocketEntity, FishingHookEntity, LargeFireballEntity, LlamaSpitEntity,
    ShulkerBulletEntity, SmallFireballEntity, SnowballEntity, SplashPotionEntity, ThrownEggEntity,
    ThrownTridentEntity, TridentPickup, WindChargeEntity, WitherSkullEntity,
};
pub use objects::vehicles::{
    BoatEntity, ChestBoatEntity, ChestMinecartEntity, ChestRaftEntity, FurnaceMinecartEntity,
    HopperMinecartEntity, MinecartEntity, RaftEntity, SpawnerMinecartEntity, TntMinecartEntity,
};
pub use objects::{
    AreaEffectCloudEntity, LightningBoltEntity, MarkerEntity, OminousItemSpawnerEntity,
    default_thunder_hit,
};
pub use raw::RawEntity;
