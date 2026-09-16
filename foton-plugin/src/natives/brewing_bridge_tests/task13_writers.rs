use super::*;
use foton_registry::data_components::DataComponentPatch;
use foton_registry::data_components::vanilla_components::USE_REMAINDER;
use foton_registry::{ItemStackTemplate, data_components::components::UseRemainder};
use foton_utils::serial::ReadFrom;
use foton_utils::serial::budget::DecodeBudget;
use foton_utils::serial::nbt_stream::writer_work;
use std::io::{self, Write};

fn chain(depth: usize) -> UseRemainder {
    DecodeBudget::new(64 * 1024 * 1024)
        .decode(|| {
            let mut template = ItemStackTemplate::try_with_count_and_patch(
                &vanilla_items::STONE,
                1,
                DataComponentPatch::new(),
            )
            .expect("terminal");
            for _ in 0..depth {
                let mut patch = DataComponentPatch::new();
                patch.set(USE_REMAINDER, UseRemainder::new(template));
                template =
                    ItemStackTemplate::try_with_count_and_patch(&vanilla_items::STONE, 1, patch)
                        .expect("template");
            }
            Ok(UseRemainder::new(template))
        })
        .expect("fixture construction")
}

fn exact_lock(data: ComponentData) -> LockCode {
    DecodeBudget::new(64 * 1024 * 1024)
        .decode(|| {
            Ok(LockCode::new(ItemPredicate::new(
                None,
                IntBounds::ANY,
                DataComponentMatchers::new(
                    DataComponentExactPredicate::new(vec![(entry("use_remainder"), data)])
                        .expect("exact"),
                    Vec::new(),
                )
                .expect("matchers"),
            )))
        })
        .expect("lock fixture")
}

#[test]
fn brewing_task13_writers_forward_linearly_on_accepted_routes() {
    init_vanilla_registry();
    let mut failures = Vec::new();
    for route in ["component", "item", "persistent_lock", "snapshot"] {
        let mut measurements = Vec::new();
        for depth in [8, 48] {
            let value = chain(depth);
            let data = ComponentData::new(value.clone());
            let mut patch = DataComponentPatch::new();
            patch.set(USE_REMAINDER, value);
            let item = ItemStack::from_raw_parts(&vanilla_items::STONE, 0, patch);
            let lock = exact_lock(data.clone());
            let mut snapshot = snapshot(vec![ItemStack::empty(); 5]);
            snapshot.lock = lock.clone();
            let mut output = Vec::new();
            let work = writer_work::measure(|| match route {
                "component" => entry("use_remainder")
                    .write_network_bounded(&data, 65536, &mut output)
                    .expect("registered network"),
                "item" => output = encode_brewing_item(&item).expect("item encode"),
                "persistent_lock" => lock.write_bounded(65536, &mut output).expect("lock encode"),
                "snapshot" => output = encode_brewing_snapshot(&snapshot).expect("snapshot encode"),
                _ => unreachable!(),
            });
            DecodeBudget::new(512 * 1024)
                .decode(|| {
                    match route {
                        "component" => {
                            entry("use_remainder")
                                .read_network(&mut Cursor::new(&output))
                                .expect("component decode");
                        }
                        "item" => {
                            assert!(read_brewing_item(&output).is_some());
                        }
                        "persistent_lock" => {
                            LockCode::read(&mut Cursor::new(&output)).expect("lock decode");
                        }
                        "snapshot" => {
                            assert!(decode_brewing_snapshot(&output).is_some());
                        }
                        _ => unreachable!(),
                    }
                    Ok(())
                })
                .expect("accepted decoder budget");
            eprintln!(
                "task13 writer {route} depth={depth} bytes={} {work:?}",
                output.len()
            );
            measurements.push(work);
        }
        let [small, large] = measurements.as_slice() else {
            unreachable!()
        };
        if large.writes > small.writes * 8 || large.metadata > small.metadata * 8 {
            failures.push(route);
        }
    }
    assert!(
        failures.is_empty(),
        "recursive adapter forwarding exceeds linear growth: {failures:?}"
    );
}

#[test]
fn brewing_task13_writer_scratch_restores_after_failure_and_unwind() {
    use foton_utils::serial::nbt_encode::NbtEncode;
    use foton_utils::serial::nbt_stream::{LimitedWriter, NbtWrite, ScratchWriter};
    use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
    init_vanilla_registry();
    let lock = exact_lock(ComponentData::new(chain(12)));
    let mut expected = Vec::new();
    lock.write_bounded(65536, &mut expected)
        .expect("canonical lock");
    let mut exact = Vec::new();
    lock.write_bounded(expected.len(), &mut exact)
        .expect("exact cap");
    assert_eq!(exact, expected);
    assert!(
        lock.write_bounded(expected.len() - 1, &mut io::sink())
            .is_err()
    );
    for unwind in [false, true] {
        let mut output = Vec::new();
        let start = {
            let mut writer = LimitedWriter::persistent(&mut output, 65536);
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                let used = writer.scratch_remaining() - 4;
                let mut reserved =
                    ScratchWriter::new(&mut writer, used).expect("outer reservation");
                if unwind {
                    resume_unwind(Box::new(()));
                }
                assert!(
                    lock.write_nbt_payload(&mut *reserved, 0).is_err(),
                    "descendant must share scratch"
                );
            }));
            assert_eq!(outcome.is_err(), unwind);
            let start = 65536 - writer.remaining();
            writer.write_all(&[10]).expect("tag");
            lock.write_nbt_payload(&mut writer, 0)
                .expect("sibling after restored reservation");
            start
        };
        assert_eq!(
            &output[start..],
            expected,
            "restoration must preserve bytes and canonical context"
        );
    }
}
