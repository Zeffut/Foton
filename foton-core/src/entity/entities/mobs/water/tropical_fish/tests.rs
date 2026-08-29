use super::*;

#[test]
fn a_pattern_packs_its_body_size_and_its_index_into_one_id() {
    // Vanilla writes `base.id | index << 8`, so the twelve patterns land on
    // twelve sparse ids rather than on 0..=11.
    assert_eq!(TropicalFishPattern::Kob.packed_id(), 0x0000);
    assert_eq!(TropicalFishPattern::Flopper.packed_id(), 0x0001);
    assert_eq!(TropicalFishPattern::Spotty.packed_id(), 0x0500);
    assert_eq!(TropicalFishPattern::Clayfish.packed_id(), 0x0501);

    for pattern in TropicalFishPattern::VALUES {
        assert_eq!(TropicalFishPattern::by_id(pattern.packed_id()), pattern);
    }

    // Vanilla's `ByIdMap.sparse` falls back to `KOB` for anything unknown.
    assert_eq!(TropicalFishPattern::by_id(0x0602), TropicalFishPattern::Kob);
}

#[test]
fn a_variant_survives_the_round_trip_through_its_packed_int() {
    // Vanilla packs pattern, base color and pattern color into one int and
    // saves only that, so a wrong shift silently recolors every fish.
    for pattern in TropicalFishPattern::VALUES {
        for base_color in DyeColor::VALUES {
            for pattern_color in DyeColor::VALUES {
                let variant = TropicalFishVariant::new(pattern, base_color, pattern_color);
                assert_eq!(
                    TropicalFishVariant::from_packed_id(variant.packed_id()),
                    variant
                );
            }
        }
    }
}

#[test]
fn the_packed_layout_matches_the_bytes_vanilla_writes() {
    let variant = TropicalFishVariant::new(
        TropicalFishPattern::Clayfish,
        DyeColor::White,
        DyeColor::Red,
    );

    let packed = variant.packed_id();

    assert_eq!(packed & 0xFFFF, TropicalFishPattern::Clayfish.packed_id());
    assert_eq!((packed >> 16) & 0xFF, DyeColor::White.id());
    assert_eq!((packed >> 24) & 0xFF, DyeColor::Red.id());
}

#[test]
fn the_default_variant_is_the_white_kob_vanilla_starts_from() {
    assert_eq!(
        TropicalFishVariant::DEFAULT,
        TropicalFishVariant::new(TropicalFishPattern::Kob, DyeColor::White, DyeColor::White)
    );
    assert_eq!(TropicalFishVariant::DEFAULT.packed_id(), 0);
}
