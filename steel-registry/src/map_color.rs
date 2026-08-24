//! Vanilla `MapColor`: the palette every pixel of a filled map is drawn from.
//!
//! Steel keeps the id and not vanilla's `col`/`modifier` RGB fields. Those two
//! only feed `MapColor.calculateARGBColor`, which runs on the client; a server
//! never turns a map pixel back into a color. All it ever writes is the packed
//! `id * 4 + brightness` byte that `ClientboundMapItemDataPacket` carries, and
//! all it ever compares is one palette entry against another.

/// One entry of vanilla's 64-slot map palette.
///
/// Vanilla parity: `net.minecraft.world.level.material.MapColor`. Equality is
/// identity there and id equality here, which is the same thing: every id in
/// the table is used at most once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapColor {
    id: u8,
}

impl MapColor {
    /// Vanilla's `MATERIAL_COLORS` array length: ids run `0..=63`.
    pub const COUNT: u8 = 64;

    pub const NONE: Self = Self::from_id(0);
    pub const GRASS: Self = Self::from_id(1);
    pub const SAND: Self = Self::from_id(2);
    pub const WOOL: Self = Self::from_id(3);
    pub const FIRE: Self = Self::from_id(4);
    pub const ICE: Self = Self::from_id(5);
    pub const METAL: Self = Self::from_id(6);
    pub const PLANT: Self = Self::from_id(7);
    pub const SNOW: Self = Self::from_id(8);
    pub const CLAY: Self = Self::from_id(9);
    pub const DIRT: Self = Self::from_id(10);
    pub const STONE: Self = Self::from_id(11);
    pub const WATER: Self = Self::from_id(12);
    pub const WOOD: Self = Self::from_id(13);
    pub const QUARTZ: Self = Self::from_id(14);
    pub const COLOR_ORANGE: Self = Self::from_id(15);
    pub const COLOR_MAGENTA: Self = Self::from_id(16);
    pub const COLOR_LIGHT_BLUE: Self = Self::from_id(17);
    pub const COLOR_YELLOW: Self = Self::from_id(18);
    pub const COLOR_LIGHT_GREEN: Self = Self::from_id(19);
    pub const COLOR_PINK: Self = Self::from_id(20);
    pub const COLOR_GRAY: Self = Self::from_id(21);
    pub const COLOR_LIGHT_GRAY: Self = Self::from_id(22);
    pub const COLOR_CYAN: Self = Self::from_id(23);
    pub const COLOR_PURPLE: Self = Self::from_id(24);
    pub const COLOR_BLUE: Self = Self::from_id(25);
    pub const COLOR_BROWN: Self = Self::from_id(26);
    pub const COLOR_GREEN: Self = Self::from_id(27);
    pub const COLOR_RED: Self = Self::from_id(28);
    pub const COLOR_BLACK: Self = Self::from_id(29);
    pub const GOLD: Self = Self::from_id(30);
    pub const DIAMOND: Self = Self::from_id(31);
    pub const LAPIS: Self = Self::from_id(32);
    pub const EMERALD: Self = Self::from_id(33);
    pub const PODZOL: Self = Self::from_id(34);
    pub const NETHER: Self = Self::from_id(35);
    pub const TERRACOTTA_WHITE: Self = Self::from_id(36);
    pub const TERRACOTTA_ORANGE: Self = Self::from_id(37);
    pub const TERRACOTTA_MAGENTA: Self = Self::from_id(38);
    pub const TERRACOTTA_LIGHT_BLUE: Self = Self::from_id(39);
    pub const TERRACOTTA_YELLOW: Self = Self::from_id(40);
    pub const TERRACOTTA_LIGHT_GREEN: Self = Self::from_id(41);
    pub const TERRACOTTA_PINK: Self = Self::from_id(42);
    pub const TERRACOTTA_GRAY: Self = Self::from_id(43);
    pub const TERRACOTTA_LIGHT_GRAY: Self = Self::from_id(44);
    pub const TERRACOTTA_CYAN: Self = Self::from_id(45);
    pub const TERRACOTTA_PURPLE: Self = Self::from_id(46);
    pub const TERRACOTTA_BLUE: Self = Self::from_id(47);
    pub const TERRACOTTA_BROWN: Self = Self::from_id(48);
    pub const TERRACOTTA_GREEN: Self = Self::from_id(49);
    pub const TERRACOTTA_RED: Self = Self::from_id(50);
    pub const TERRACOTTA_BLACK: Self = Self::from_id(51);
    pub const CRIMSON_NYLIUM: Self = Self::from_id(52);
    pub const CRIMSON_STEM: Self = Self::from_id(53);
    pub const CRIMSON_HYPHAE: Self = Self::from_id(54);
    pub const WARPED_NYLIUM: Self = Self::from_id(55);
    pub const WARPED_STEM: Self = Self::from_id(56);
    pub const WARPED_HYPHAE: Self = Self::from_id(57);
    pub const WARPED_WART_BLOCK: Self = Self::from_id(58);
    pub const DEEPSLATE: Self = Self::from_id(59);
    pub const RAW_IRON: Self = Self::from_id(60);
    pub const GLOW_LICHEN: Self = Self::from_id(61);

    /// Builds a palette entry from its vanilla id.
    ///
    /// # Panics
    /// Panics if `id` is outside vanilla's `0..=63` range, matching the
    /// `IndexOutOfBoundsException` the vanilla constructor throws.
    #[must_use]
    pub const fn from_id(id: u8) -> Self {
        assert!(id < Self::COUNT, "map color id must be between 0 and 63");
        Self { id }
    }

    #[must_use]
    pub const fn id(self) -> u8 {
        self.id
    }

    /// Vanilla parity: `MapColor.getPackedId`.
    ///
    /// Steel returns `u8` where vanilla returns `byte`; the map's color array
    /// and the packet's patch are both opaque bytes, so the signedness is not
    /// observable.
    #[must_use]
    pub const fn packed_id(self, brightness: Brightness) -> u8 {
        (self.id << 2) | (brightness.id() & 3)
    }

    /// Splits a packed map byte back into its palette entry and brightness.
    ///
    /// Vanilla parity: the `val >> 2` / `val & 3` halves of
    /// `MapColor.getColorFromPackedId`.
    #[must_use]
    pub const fn unpack(packed: u8) -> (Self, Brightness) {
        (Self::from_id(packed >> 2), Brightness::from_id(packed & 3))
    }
}

/// The four shades a palette entry can be drawn in.
///
/// Vanilla parity: `MapColor.Brightness`. Vanilla's `modifier` field is left
/// out for the same reason as `MapColor.col` -- it only scales RGB on the
/// client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Brightness {
    Low,
    Normal,
    High,
    Lowest,
}

impl Brightness {
    #[must_use]
    pub const fn id(self) -> u8 {
        match self {
            Self::Low => 0,
            Self::Normal => 1,
            Self::High => 2,
            Self::Lowest => 3,
        }
    }

    /// Vanilla parity: `MapColor.Brightness.byId`.
    ///
    /// # Panics
    /// Panics outside `0..=3`, matching vanilla's `checkPositionIndex`.
    #[must_use]
    pub const fn from_id(id: u8) -> Self {
        match id {
            0 => Self::Low,
            1 => Self::Normal,
            2 => Self::High,
            3 => Self::Lowest,
            _ => panic!("brightness id must be between 0 and 3"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Brightness, MapColor};

    /// The packed byte is what a client reads a map pixel out of, so a wrong
    /// shift or mask would render the whole map in the wrong palette.
    #[test]
    fn a_packed_pixel_round_trips_through_its_color_and_shade() {
        for id in 0..MapColor::COUNT {
            for brightness in [
                Brightness::Low,
                Brightness::Normal,
                Brightness::High,
                Brightness::Lowest,
            ] {
                let color = MapColor::from_id(id);
                let packed = color.packed_id(brightness);
                assert_eq!(packed, id * 4 + brightness.id());
                assert_eq!(MapColor::unpack(packed), (color, brightness));
            }
        }
    }
}
