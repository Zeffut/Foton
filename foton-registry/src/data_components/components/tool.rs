//! Tool component for mining speed and drop behavior.

use crate::data_components::registry::ValidatePersistentComponent;
use foton_utils::serial::budget;
use foton_utils::serial::nbt_encode;
use foton_utils::serial::nbt_stream::NbtWrite;
use std::io;
use std::io::{Cursor, Error, Result, Write};

#[cfg(test)]
mod validation_work {
    use std::cell::Cell;

    thread_local! { static VALIDATIONS: Cell<usize> = const { Cell::new(0) }; }

    pub(super) fn visit() {
        VALIDATIONS.set(VALIDATIONS.get() + 1);
    }

    pub(super) fn measure(operation: impl FnOnce()) -> usize {
        VALIDATIONS.set(0);
        operation();
        VALIDATIONS.get()
    }
}

use foton_utils::{
    BlockStateId,
    codec::VarInt,
    hash::{ComponentHasher, HashComponent, HashEntry, sort_map_entries},
    nbt::NbtNumeric as _,
    serial::{ReadFrom, WriteTo},
};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use simdnbt::{FromNbtTag, ToNbtTag};

use crate::blocks::Block;
use crate::{REGISTRY, RegistryHolderSet};

/// The block holder set used by a tool rule.
pub type ToolRuleBlocks = RegistryHolderSet<Block>;

/// A single rule within a tool component.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolRule {
    /// Blocks to which this rule applies.
    pub blocks: ToolRuleBlocks,
    /// Mining speed, or the tool's default speed when absent.
    pub speed: Option<f32>,
    /// Whether matching blocks drop items, when explicitly specified.
    pub correct_for_drops: Option<bool>,
}

impl ToolRule {
    /// Creates a rule that sets both mining speed and correct-tool behavior.
    #[must_use]
    pub const fn mines_and_drops(blocks: ToolRuleBlocks, speed: f32) -> Self {
        Self {
            blocks,
            speed: Some(speed),
            correct_for_drops: Some(true),
        }
    }

    /// Creates a rule that explicitly denies drops.
    #[must_use]
    pub const fn denies_drops(blocks: ToolRuleBlocks) -> Self {
        Self {
            blocks,
            speed: None,
            correct_for_drops: Some(false),
        }
    }

    /// Creates a rule that only overrides mining speed.
    #[must_use]
    pub const fn override_speed(blocks: ToolRuleBlocks, speed: f32) -> Self {
        Self {
            blocks,
            speed: Some(speed),
            correct_for_drops: None,
        }
    }

    /// Returns whether this rule matches a block state.
    #[must_use]
    pub fn matches_block(&self, block_state_id: BlockStateId) -> bool {
        REGISTRY
            .blocks
            .by_state_id(block_state_id)
            .is_some_and(|block| self.blocks.contains(block))
    }
}

/// Mining speed and drop behavior for an item.
#[derive(Debug, Clone, PartialEq)]
pub struct Tool {
    /// Rules evaluated in order.
    pub rules: Vec<ToolRule>,
    /// Mining speed when no rule with a speed matches.
    pub default_mining_speed: f32,
    /// Item damage applied per block mined.
    pub damage_per_block: i32,
    /// Whether the tool may destroy blocks in creative mode.
    pub can_destroy_blocks_in_creative: bool,
}

impl Default for Tool {
    fn default() -> Self {
        Self {
            rules: Vec::new(),
            default_mining_speed: 1.0,
            damage_per_block: 1,
            can_destroy_blocks_in_creative: true,
        }
    }
}

impl Tool {
    /// Returns the first matching rule speed, or the default speed.
    #[must_use]
    pub fn get_mining_speed(&self, block_state_id: BlockStateId) -> f32 {
        for rule in &self.rules {
            if let Some(speed) = rule.speed
                && rule.matches_block(block_state_id)
            {
                return speed;
            }
        }
        self.default_mining_speed
    }

    /// Returns the first explicitly specified correct-tool result.
    #[must_use]
    pub fn is_correct_for_drops(&self, block_state_id: BlockStateId) -> bool {
        for rule in &self.rules {
            if let Some(correct) = rule.correct_for_drops
                && rule.matches_block(block_state_id)
            {
                return correct;
            }
        }
        false
    }
}

impl WriteTo for ToolRule {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        self.blocks.write(writer)?;
        self.speed.write(writer)?;
        self.correct_for_drops.write(writer)
    }
}

impl ReadFrom for ToolRule {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        Ok(Self {
            blocks: ToolRuleBlocks::read(data)?,
            speed: Option::<f32>::read(data)?,
            correct_for_drops: Option::<bool>::read(data)?,
        })
    }
}

impl WriteTo for Tool {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        let count = i32::try_from(self.rules.len())
            .map_err(|_| Error::other(format!("Tool rule list too large: {}", self.rules.len())))?;
        VarInt(count).write(writer)?;
        for rule in &self.rules {
            rule.write(writer)?;
        }
        self.default_mining_speed.write(writer)?;
        VarInt(self.damage_per_block).write(writer)?;
        self.can_destroy_blocks_in_creative.write(writer)
    }
}

impl ReadFrom for Tool {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        let count = VarInt::read(data)?.0;
        let count = usize::try_from(count)
            .map_err(|_| Error::other(format!("Negative tool rule count: {count}")))?;
        budget::check_collection_input(data, count, 3)?;
        let mut rules = budget::read_vec(count, count.min(65_536))?;
        for _ in 0..count {
            rules.push(ToolRule::read(data)?);
        }
        Ok(Self {
            rules,
            default_mining_speed: f32::read(data)?,
            damage_per_block: VarInt::read(data)?.0,
            can_destroy_blocks_in_creative: bool::read(data)?,
        })
    }
}

impl ToNbtTag for ToolRule {
    fn to_nbt_tag(self) -> NbtTag {
        NbtTag::Compound(self.into_nbt_compound())
    }
}

impl ToolRule {
    fn into_nbt_compound(self) -> NbtCompound {
        let mut compound = NbtCompound::new();
        compound.insert("blocks", self.blocks.to_nbt_tag());
        if let Some(speed) = self.speed {
            compound.insert("speed", speed);
        }
        if let Some(correct_for_drops) = self.correct_for_drops {
            compound.insert("correct_for_drops", i8::from(correct_for_drops));
        }
        compound
    }

    fn from_nbt_compound(compound: simdnbt::borrow::NbtCompound<'_, '_>) -> Option<Self> {
        let blocks = ToolRuleBlocks::from_nbt_tag(compound.get("blocks")?)?;
        let speed = match compound.get("speed") {
            Some(tag) => {
                let speed = tag.codec_f32()?;
                if !speed.is_finite() || speed <= 0.0 {
                    return None;
                }
                Some(speed)
            }
            None => None,
        };
        let correct_for_drops = match compound.get("correct_for_drops") {
            Some(tag) => Some(tag.codec_bool()?),
            None => None,
        };
        Some(Self {
            blocks,
            speed,
            correct_for_drops,
        })
    }
}

impl FromNbtTag for ToolRule {
    fn from_nbt_tag(tag: simdnbt::borrow::NbtTag) -> Option<Self> {
        Self::from_nbt_compound(tag.compound()?)
    }
}

impl ToNbtTag for Tool {
    fn to_nbt_tag(self) -> NbtTag {
        let mut compound = NbtCompound::new();
        compound.insert(
            "rules",
            NbtList::Compound(
                self.rules
                    .into_iter()
                    .map(ToolRule::into_nbt_compound)
                    .collect(),
            ),
        );
        if self.default_mining_speed.to_bits() != 1.0_f32.to_bits() {
            compound.insert("default_mining_speed", self.default_mining_speed);
        }
        if self.damage_per_block != 1 {
            compound.insert("damage_per_block", self.damage_per_block);
        }
        if !self.can_destroy_blocks_in_creative {
            compound.insert(
                "can_destroy_blocks_in_creative",
                i8::from(self.can_destroy_blocks_in_creative),
            );
        }
        NbtTag::Compound(compound)
    }
}

impl FromNbtTag for Tool {
    fn from_nbt_tag(tag: simdnbt::borrow::NbtTag) -> Option<Self> {
        let compound = tag.compound()?;
        let rule_compounds = compound.get("rules")?.list()?.compounds()?;
        let mut rules = Vec::with_capacity(rule_compounds.len());
        for rule in rule_compounds {
            rules.push(ToolRule::from_nbt_compound(rule)?);
        }
        let default_mining_speed = match compound.get("default_mining_speed") {
            Some(tag) => tag.codec_f32()?,
            None => 1.0,
        };
        let damage_per_block = match compound.get("damage_per_block") {
            Some(tag) => tag.codec_i32()?,
            None => 1,
        };
        if damage_per_block < 0 {
            return None;
        }
        let can_destroy_blocks_in_creative = match compound.get("can_destroy_blocks_in_creative") {
            Some(tag) => tag.codec_bool()?,
            None => true,
        };
        Some(Self {
            rules,
            default_mining_speed,
            damage_per_block,
            can_destroy_blocks_in_creative,
        })
    }
}

impl HashComponent for ToolRule {
    fn hash_component(&self, hasher: &mut ComponentHasher) {
        let mut entries = Vec::new();
        push_hash_entry(&mut entries, "blocks", &self.blocks);
        if let Some(speed) = self.speed {
            push_hash_entry(&mut entries, "speed", &speed);
        }
        if let Some(correct_for_drops) = self.correct_for_drops {
            push_hash_entry(&mut entries, "correct_for_drops", &correct_for_drops);
        }
        hash_entries(hasher, &mut entries);
    }
}

impl HashComponent for Tool {
    fn hash_component(&self, hasher: &mut ComponentHasher) {
        let mut entries = Vec::new();
        push_hash_list_entry(&mut entries, "rules", &self.rules);
        if self.default_mining_speed.to_bits() != 1.0_f32.to_bits() {
            push_hash_entry(
                &mut entries,
                "default_mining_speed",
                &self.default_mining_speed,
            );
        }
        if self.damage_per_block != 1 {
            push_hash_entry(&mut entries, "damage_per_block", &self.damage_per_block);
        }
        if !self.can_destroy_blocks_in_creative {
            push_hash_entry(
                &mut entries,
                "can_destroy_blocks_in_creative",
                &self.can_destroy_blocks_in_creative,
            );
        }
        hash_entries(hasher, &mut entries);
    }
}

fn hash_entries(hasher: &mut ComponentHasher, entries: &mut [HashEntry]) {
    sort_map_entries(entries);
    hasher.start_map();
    for entry in entries {
        hasher.put_raw_bytes(&entry.key_bytes);
        hasher.put_raw_bytes(&entry.value_bytes);
    }
    hasher.end_map();
}

fn push_hash_entry<T: HashComponent + ?Sized>(entries: &mut Vec<HashEntry>, key: &str, value: &T) {
    let mut key_hasher = ComponentHasher::new();
    key_hasher.put_string(key);
    let mut value_hasher = ComponentHasher::new();
    value.hash_component(&mut value_hasher);
    entries.push(HashEntry::new(key_hasher, value_hasher));
}

fn push_hash_list_entry(entries: &mut Vec<HashEntry>, key: &str, values: &[ToolRule]) {
    let mut key_hasher = ComponentHasher::new();
    key_hasher.put_string(key);
    let mut value_hasher = ComponentHasher::new();
    value_hasher.start_list();
    for value in values {
        value_hasher.put_component_hash(value);
    }
    value_hasher.end_list();
    entries.push(HashEntry::new(key_hasher, value_hasher));
}

impl ValidatePersistentComponent for ToolRule {
    fn validate_persistent(&self) -> Result<()> {
        #[cfg(test)]
        validation_work::visit();
        if self
            .speed
            .is_some_and(|speed| !speed.is_finite() || speed <= 0.0)
        {
            return Err(Error::other("Invalid persistent tool speed"));
        }
        Ok(())
    }
}

impl nbt_encode::NbtEncode for ToolRule {
    fn nbt_id(&self) -> u8 {
        10
    }
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, depth: usize) -> io::Result<()> {
        use foton_utils::serial::nbt_encode::{check_depth, end, field};
        check_depth(depth)?;

        self.validate_persistent()?;
        field("blocks", &(self.blocks), writer, depth)?;
        if let Some(speed) = &self.speed {
            field("speed", &(speed), writer, depth)?;
        }
        if let Some(correct_for_drops) = &self.correct_for_drops {
            field("correct_for_drops", correct_for_drops, writer, depth)?;
        }
        end(writer)
    }
}

impl ValidatePersistentComponent for Tool {
    fn validate_persistent(&self) -> Result<()> {
        if self.damage_per_block < 0 {
            return Err(Error::other("Invalid persistent tool damage"));
        }
        for rule in &self.rules {
            rule.validate_persistent()?;
        }
        Ok(())
    }
}

impl nbt_encode::NbtEncode for Tool {
    fn nbt_id(&self) -> u8 {
        10
    }
    fn write_nbt_payload(&self, writer: &mut dyn NbtWrite, depth: usize) -> io::Result<()> {
        use foton_utils::serial::nbt_encode::{check_depth, end, field};
        check_depth(depth)?;
        writer.ensure_remaining(self.rules.len().saturating_add(5))?;
        if self.damage_per_block < 0 {
            return Err(Error::other("Invalid persistent tool damage"));
        }
        field("rules", &self.rules, writer, depth)?;
        if self.default_mining_speed.to_bits() != 1.0_f32.to_bits() {
            field(
                "default_mining_speed",
                &self.default_mining_speed,
                writer,
                depth,
            )?;
        }
        if self.damage_per_block != 1 {
            field("damage_per_block", &self.damage_per_block, writer, depth)?;
        }
        if !self.can_destroy_blocks_in_creative {
            field("can_destroy_blocks_in_creative", &false, writer, depth)?;
        }
        end(writer)
    }
}

#[cfg(test)]
mod tests {
    use std::io::{self, Cursor};

    use foton_utils::Identifier;
    use foton_utils::hash::HashComponent;
    use foton_utils::serial::{ReadFrom, WriteTo};
    use simdnbt::borrow::{NbtTag as BorrowedNbtTag, read_tag};
    use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
    use simdnbt::{FromNbtTag, ToNbtTag};

    use super::{Tool, ToolRule, ToolRuleBlocks};
    use crate::data_component_predicate::{DataComponentExactPredicate, DataComponentMatchers};
    use crate::data_components::{ComponentData, vanilla_components::TOOL};
    use crate::item_predicate::{IntBounds, ItemPredicate, LockCode};
    use crate::vanilla_blocks::{COBWEB, STONE};
    use crate::{REGISTRY, RegistryExt, init_vanilla_registry};

    fn with_borrowed_tag<R>(tag: NbtTag, visitor: impl FnOnce(BorrowedNbtTag<'_, '_>) -> R) -> R {
        let mut bytes = Vec::new();
        tag.write(&mut bytes);
        let borrowed =
            read_tag(&mut Cursor::new(bytes.as_slice())).expect("owned test tag should parse");
        visitor(borrowed.as_tag())
    }

    fn parse_tool(tag: NbtTag) -> Option<Tool> {
        with_borrowed_tag(tag, Tool::from_nbt_tag)
    }

    fn sample_tool() -> Tool {
        Tool {
            rules: vec![
                ToolRule {
                    blocks: ToolRuleBlocks::Tag(Identifier::vanilla_static("mineable/pickaxe")),
                    speed: Some(4.0),
                    correct_for_drops: Some(true),
                },
                ToolRule {
                    blocks: ToolRuleBlocks::Direct(vec![&COBWEB, &STONE]),
                    speed: Some(15.0),
                    correct_for_drops: None,
                },
            ],
            default_mining_speed: 1.5,
            damage_per_block: 2,
            can_destroy_blocks_in_creative: false,
        }
    }

    #[test]
    fn brewing_task13_persistent_lock_tiny_cap_precedes_rule_validation() {
        init_vanilla_registry();
        let rule = ToolRule::override_speed(ToolRuleBlocks::Direct(vec![]), 1.0);
        let tool = Tool {
            rules: vec![rule; 500_000],
            ..Tool::default()
        };
        let entry = REGISTRY
            .data_components
            .by_key(TOOL.key())
            .expect("tool component");
        let exact = DataComponentExactPredicate::new(vec![(entry, ComponentData::new(tool))])
            .expect("valid tool");
        let lock = LockCode::new(ItemPredicate::new(
            None,
            IntBounds::ANY,
            DataComponentMatchers::new(exact, vec![]).expect("matchers"),
        ));
        let mut stats = None;
        let validations = super::validation_work::measure(|| {
            stats = Some(allocation_counter::measure(|| {
                assert!(lock.write_bounded(96, &mut io::sink()).is_err());
            }));
        });
        assert_eq!(validations, 0, "tiny cap validated persistent tool rules");
        let stats = stats.expect("allocation stats");
        assert!(stats.bytes_max < 4096, "{stats:?}");
    }

    #[test]
    fn tool_network_round_trips_tag_and_direct_holder_sets() {
        init_vanilla_registry();
        let tool = sample_tool();
        let mut bytes = Vec::new();
        tool.write(&mut bytes).expect("tool should serialize");

        let decoded =
            Tool::read(&mut Cursor::new(bytes.as_slice())).expect("tool should deserialize");

        assert_eq!(decoded, tool);
    }

    #[test]
    fn tool_nbt_uses_compact_holder_sets_and_numeric_coercion() {
        init_vanilla_registry();
        let mut rule = NbtCompound::new();
        rule.insert("blocks", "minecraft:cobweb");
        rule.insert("speed", 5.5_f64);
        rule.insert("correct_for_drops", 1_i32);
        let mut compound = NbtCompound::new();
        compound.insert("rules", NbtList::Compound(vec![rule]));
        compound.insert("damage_per_block", 2_i8);

        let parsed = parse_tool(NbtTag::Compound(compound)).expect("valid tool should parse");

        assert_eq!(
            parsed.rules[0].blocks,
            ToolRuleBlocks::Direct(vec![&COBWEB])
        );
        assert_eq!(parsed.rules[0].speed, Some(5.5));
        assert_eq!(parsed.rules[0].correct_for_drops, Some(true));
        assert_eq!(parsed.damage_per_block, 2);

        let NbtTag::Compound(encoded) = parsed.to_nbt_tag() else {
            panic!("tool should encode as a compound");
        };
        let rules = encoded
            .get("rules")
            .and_then(|tag| match tag {
                NbtTag::List(NbtList::Compound(rules)) => Some(rules),
                _ => None,
            })
            .expect("tool rules should encode as a compound list");
        assert_eq!(
            rules[0].get("blocks"),
            Some(&NbtTag::String("minecraft:cobweb".into()))
        );
    }

    #[test]
    fn malformed_present_tool_fields_fail_the_codec() {
        init_vanilla_registry();

        assert!(parse_tool(NbtTag::Compound(NbtCompound::new())).is_none());

        let mut bad_rule = NbtCompound::new();
        bad_rule.insert("blocks", "minecraft:cobweb");
        bad_rule.insert("speed", "fast");
        let mut compound = NbtCompound::new();
        compound.insert("rules", NbtList::Compound(vec![bad_rule]));
        assert!(parse_tool(NbtTag::Compound(compound)).is_none());

        let mut bad_rule = NbtCompound::new();
        bad_rule.insert("blocks", "minecraft:not_a_block");
        let mut compound = NbtCompound::new();
        compound.insert("rules", NbtList::Compound(vec![bad_rule]));
        assert!(parse_tool(NbtTag::Compound(compound)).is_none());
    }

    #[test]
    fn tool_hash_matches_its_persistent_codec_shape() {
        init_vanilla_registry();
        let tool = Tool {
            rules: vec![ToolRule {
                blocks: ToolRuleBlocks::Direct(vec![&COBWEB]),
                speed: Some(4.0),
                correct_for_drops: None,
            }],
            ..Tool::default()
        };
        let expected = tool.clone().to_nbt_tag().compute_hash();

        assert_eq!(tool.compute_hash(), expected);

        let mut with_correct_for_drops = tool.clone();
        with_correct_for_drops.rules[0].correct_for_drops = Some(true);
        assert_ne!(tool.compute_hash(), with_correct_for_drops.compute_hash());
    }
}
