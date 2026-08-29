//! The coat and markings a horse is born with.
//!
//! Vanilla parity: `animal.equine.Variant` and `animal.equine.Markings`. Both
//! travel as one synchronized int, the coat in the low byte and the markings in
//! the high one, which is why they are defined together.

/// The seven horse coats.
///
/// Vanilla parity: `animal.equine.Variant`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HorseVariant {
    /// Vanilla `Variant.WHITE`.
    #[default]
    White,
    /// Vanilla `Variant.CREAMY`.
    Creamy,
    /// Vanilla `Variant.CHESTNUT`.
    Chestnut,
    /// Vanilla `Variant.BROWN`.
    Brown,
    /// Vanilla `Variant.BLACK`.
    Black,
    /// Vanilla `Variant.GRAY`.
    Gray,
    /// Vanilla `Variant.DARK_BROWN`.
    DarkBrown,
}

impl HorseVariant {
    /// Every coat in vanilla id order.
    pub const ALL: [Self; 7] = [
        Self::White,
        Self::Creamy,
        Self::Chestnut,
        Self::Brown,
        Self::Black,
        Self::Gray,
        Self::DarkBrown,
    ];

    /// Returns the vanilla synchronized id.
    #[must_use]
    pub const fn id(self) -> i32 {
        match self {
            Self::White => 0,
            Self::Creamy => 1,
            Self::Chestnut => 2,
            Self::Brown => 3,
            Self::Black => 4,
            Self::Gray => 5,
            Self::DarkBrown => 6,
        }
    }

    /// Returns the coat for a synchronized id.
    ///
    /// Vanilla parity: `Variant.BY_ID`, which wraps rather than clamps.
    #[must_use]
    pub const fn by_id(id: i32) -> Self {
        let len = Self::ALL.len() as i32;
        Self::ALL[id.rem_euclid(len) as usize]
    }

    /// Picks a coat at random, as vanilla's `Util.getRandom` does.
    #[must_use]
    pub fn random() -> Self {
        Self::ALL[rand::random_range(0..Self::ALL.len())]
    }
}

/// The five sets of markings laid over a coat.
///
/// Vanilla parity: `animal.equine.Markings`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HorseMarkings {
    /// Vanilla `Markings.NONE`.
    #[default]
    None,
    /// Vanilla `Markings.WHITE`.
    White,
    /// Vanilla `Markings.WHITE_FIELD`.
    WhiteField,
    /// Vanilla `Markings.WHITE_DOTS`.
    WhiteDots,
    /// Vanilla `Markings.BLACK_DOTS`.
    BlackDots,
}

impl HorseMarkings {
    /// Every marking in vanilla id order.
    pub const ALL: [Self; 5] = [
        Self::None,
        Self::White,
        Self::WhiteField,
        Self::WhiteDots,
        Self::BlackDots,
    ];

    /// Returns the vanilla synchronized id.
    #[must_use]
    pub const fn id(self) -> i32 {
        match self {
            Self::None => 0,
            Self::White => 1,
            Self::WhiteField => 2,
            Self::WhiteDots => 3,
            Self::BlackDots => 4,
        }
    }

    /// Returns the marking for a synchronized id.
    ///
    /// Vanilla parity: `Markings.BY_ID`, which wraps rather than clamps.
    #[must_use]
    pub const fn by_id(id: i32) -> Self {
        let len = Self::ALL.len() as i32;
        Self::ALL[id.rem_euclid(len) as usize]
    }

    /// Picks markings at random, as vanilla's `Util.getRandom` does.
    #[must_use]
    pub fn random() -> Self {
        Self::ALL[rand::random_range(0..Self::ALL.len())]
    }
}

/// Packs a coat and its markings into vanilla's single synchronized int.
///
/// Vanilla parity: `Horse.setVariantAndMarkings`.
#[must_use]
pub const fn pack_type_variant(variant: HorseVariant, markings: HorseMarkings) -> i32 {
    variant.id() & 0xFF | markings.id() << 8 & 0xFF00
}

/// Replaces only the coat of a packed value.
///
/// Vanilla parity: `Horse.setVariant`.
#[must_use]
pub const fn with_variant(type_variant: i32, variant: HorseVariant) -> i32 {
    variant.id() & 0xFF | type_variant & -256
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_packed_coat_and_marking_survive_the_round_trip() {
        // The two halves share one int, so an off-by-one shift would silently
        // repaint every horse on the server.
        for variant in HorseVariant::ALL {
            for markings in HorseMarkings::ALL {
                let packed = pack_type_variant(variant, markings);
                assert_eq!(HorseVariant::by_id(packed & 0xFF), variant);
                assert_eq!(HorseMarkings::by_id((packed & 0xFF00) >> 8), markings);
            }
        }
    }

    #[test]
    fn replacing_the_coat_leaves_the_markings_alone() {
        let packed = pack_type_variant(HorseVariant::Black, HorseMarkings::WhiteDots);
        let repainted = with_variant(packed, HorseVariant::Creamy);

        assert_eq!(HorseVariant::by_id(repainted & 0xFF), HorseVariant::Creamy);
        assert_eq!(
            HorseMarkings::by_id((repainted & 0xFF00) >> 8),
            HorseMarkings::WhiteDots
        );
    }

    #[test]
    fn out_of_range_ids_wrap_like_vanillas_continuous_map() {
        assert_eq!(HorseVariant::by_id(7), HorseVariant::White);
        assert_eq!(HorseVariant::by_id(-1), HorseVariant::DarkBrown);
        assert_eq!(HorseMarkings::by_id(5), HorseMarkings::None);
    }
}
