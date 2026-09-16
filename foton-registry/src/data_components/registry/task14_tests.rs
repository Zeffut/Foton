use crate::data_component_predicate::{
    DataComponentExactPredicate, DataComponentMatchers, DataComponentPredicateData,
};
use crate::data_components::components::UseRemainder;
use crate::data_components::vanilla_components::USE_REMAINDER;
use crate::data_components::{ComponentData, ComponentEntryRef, DataComponentPatch};
use crate::item_predicate::{AdventureModePredicate, BlockPredicate};
use crate::{ItemStackTemplate, REGISTRY, RegistryExt, init_vanilla_registry, vanilla_items};
use foton_utils::Identifier;
use foton_utils::serial::nbt_stream::{LimitedWriter, NbtWrite};
use std::io::{self, Write};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

fn entry(name: &'static str) -> ComponentEntryRef {
    REGISTRY
        .data_components
        .by_key(&Identifier::vanilla_static(name))
        .expect("entry")
}
fn template_chain(depth: usize) -> ComponentData {
    let mut template = ItemStackTemplate::try_with_count_and_patch(
        &vanilla_items::STONE,
        1,
        DataComponentPatch::new(),
    )
    .expect("leaf");
    for _ in 0..depth {
        let mut patch = DataComponentPatch::new();
        patch.set(USE_REMAINDER, UseRemainder::new(template));
        template = ItemStackTemplate::try_with_count_and_patch(&vanilla_items::STONE, 1, patch)
            .expect("template");
    }
    ComponentData::new(UseRemainder::new(template))
}
fn exact_chain(depth: usize) -> ComponentData {
    let mut value = AdventureModePredicate::new(vec![BlockPredicate::new(
        None,
        None,
        None,
        DataComponentMatchers::ANY,
    )])
    .expect("leaf");
    for _ in 0..depth {
        let exact =
            DataComponentExactPredicate::new(vec![(entry("can_break"), ComponentData::new(value))])
                .expect("exact");
        value = AdventureModePredicate::new(vec![BlockPredicate::new(
            None,
            None,
            None,
            DataComponentMatchers::new(exact, vec![]).expect("matchers"),
        )])
        .expect("adventure");
    }
    ComponentData::new(value)
}
#[test]
fn task14_network_retained_indices_share_scratch_and_restore() {
    init_vanilla_registry();
    let mut failures = Vec::new();
    for (name, data) in [
        ("use_remainder", template_chain(8)),
        ("can_break", exact_chain(12)),
    ] {
        let mut sink = io::sink();
        let mut writer = LimitedWriter::new(&mut sink, 65536);
        writer.set_scratch_remaining(64);
        let result = entry(name).write_network_in(&data, &mut writer);
        eprintln!(
            "task14 scratch {name}: result={result:?}, restored={}",
            writer.scratch_remaining()
        );
        if result.is_ok() {
            failures.push(name);
        }
        assert_eq!(writer.scratch_remaining(), 64);
        writer.set_scratch_remaining(384 * 1024);
        assert!(entry(name).write_network_in(&data, &mut writer).is_ok());
    }
    let partial = DataComponentMatchers::new(
        DataComponentExactPredicate::EMPTY,
        vec![
            DataComponentPredicateData::any(entry("profile")),
            DataComponentPredicateData::any(entry("custom_name")),
        ],
    )
    .expect("partial");
    let mut sink = io::sink();
    let mut writer = LimitedWriter::new(&mut sink, 65536);
    writer.set_scratch_remaining(0);
    if partial.write_bounded(&mut writer).is_ok() {
        failures.push("partial");
    }
    assert_eq!(writer.scratch_remaining(), 0);
    assert!(
        failures.is_empty(),
        "unaccounted retained index: {failures:?}"
    );
}

struct ObservingWriter {
    scratch: usize,
    minimum_observed: usize,
    writes_before_failure: usize,
    unwind: bool,
}
impl Write for ObservingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.minimum_observed = self.minimum_observed.min(self.scratch);
        if self.writes_before_failure == 0 {
            if self.unwind {
                resume_unwind(Box::new(()));
            }
            return Err(io::Error::other("injected child emission failure"));
        }
        self.writes_before_failure -= 1;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl NbtWrite for ObservingWriter {
    fn remaining(&self) -> usize {
        65536
    }
    fn container_limit(&self) -> usize {
        512
    }
    fn canonical_compounds(&self) -> bool {
        true
    }
    fn scratch_remaining(&self) -> usize {
        self.scratch
    }
    fn set_scratch_remaining(&mut self, remaining: usize) {
        self.scratch = remaining;
    }
}
#[test]
fn task14_network_scratch_unwind_restores() {
    init_vanilla_registry();
    for (name, data) in [
        ("use_remainder", template_chain(4)),
        ("can_break", exact_chain(4)),
    ] {
        for unwind in [false, true] {
            let mut writer = ObservingWriter {
                scratch: 1234,
                minimum_observed: 1234,
                writes_before_failure: 12,
                unwind,
            };
            let result = catch_unwind(AssertUnwindSafe(|| {
                entry(name).write_network_in(&data, &mut writer)
            }));
            if unwind {
                assert!(result.is_err());
            } else {
                assert!(result.expect("ordinary error").is_err());
            }
            // The fault happens inside descendant emission with multiple ancestor indices live.
            assert!(writer.minimum_observed <= 1234 - 2 * size_of::<usize>());
            assert_eq!(writer.scratch_remaining(), 1234);
        }
    }
}
