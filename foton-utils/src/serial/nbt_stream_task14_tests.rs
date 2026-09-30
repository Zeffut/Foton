use super::*;

#[test]
fn brewing_task14_raw_float_writers_match_java_bits_and_exact_caps() {
    for (input, expected) in [
        (0x7fc0_0001_u32, 0x7fc0_0000_u32),
        (0xff80_0001, 0x7fc0_0000),
        (0x7fc0_0000, 0x7fc0_0000),
        (0, 0),
        (0x8000_0000, 0x8000_0000),
        (0x7f80_0000, 0x7f80_0000),
        (0xff80_0000, 0xff80_0000),
        (1, 1),
        (0xc2f6_8000, 0xc2f6_8000),
    ] {
        for list in [false, true] {
            let value = f32::from_bits(input);
            let tag = if list {
                NbtTag::List(NbtList::Float(vec![value]))
            } else {
                NbtTag::Float(value)
            };
            let mut wire = if list {
                vec![9, 5, 0, 0, 0, 1]
            } else {
                vec![5]
            };
            wire.extend_from_slice(&expected.to_be_bytes());
            assert_wire(&tag, &wire);
        }
    }
    for (input, expected) in [
        (0x7ff8_0000_0000_0001_u64, 0x7ff8_0000_0000_0000_u64),
        (0xfff0_0000_0000_0001, 0x7ff8_0000_0000_0000),
        (0x7ff8_0000_0000_0000, 0x7ff8_0000_0000_0000),
        (0, 0),
        (0x8000_0000_0000_0000, 0x8000_0000_0000_0000),
        (0x7ff0_0000_0000_0000, 0x7ff0_0000_0000_0000),
        (0xfff0_0000_0000_0000, 0xfff0_0000_0000_0000),
        (1, 1),
        (0xc05e_dd2f_1a9f_be77, 0xc05e_dd2f_1a9f_be77),
    ] {
        for list in [false, true] {
            let value = f64::from_bits(input);
            let tag = if list {
                NbtTag::List(NbtList::Double(vec![value]))
            } else {
                NbtTag::Double(value)
            };
            let mut wire = if list {
                vec![9, 6, 0, 0, 0, 1]
            } else {
                vec![6]
            };
            wire.extend_from_slice(&expected.to_be_bytes());
            assert_wire(&tag, &wire);
        }
    }
}
fn assert_wire(tag: &NbtTag, expected: &[u8]) {
    let mut actual = Vec::new();
    write_tag(tag, &mut LimitedWriter::new(&mut actual, expected.len()), 0).expect("exact cap");
    assert_eq!(actual, expected);
    assert!(
        write_tag(
            tag,
            &mut LimitedWriter::new(&mut sink(), expected.len() - 1),
            0
        )
        .is_err()
    );
}
