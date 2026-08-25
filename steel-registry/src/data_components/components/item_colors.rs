//! Vanilla item color and map ID components.

use std::io::{Cursor, Result, Write};

use simdnbt::owned::NbtTag;
use simdnbt::{FromNbtTag, ToNbtTag};
use steel_utils::codec::VarInt;
use steel_utils::hash::{ComponentHasher, HashComponent};
use steel_utils::nbt::NbtNumeric as _;
use steel_utils::serial::{ReadFrom, WriteTo};

use super::rgb_color::decode_rgb_color;

/// RGB color applied to dyeable items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DyedItemColor {
    rgb: i32,
}

impl DyedItemColor {
    pub const LEATHER_COLOR: i32 = -6_265_536;

    #[must_use]
    pub const fn new(rgb: i32) -> Self {
        Self { rgb }
    }

    #[must_use]
    pub const fn rgb(self) -> i32 {
        self.rgb
    }

    /// Mixes `dyes` into the color an item already carries.
    ///
    /// Vanilla parity: `DyedItemColor.applyDyes(DyedItemColor, List<DyeColor>)`.
    /// The channels are averaged, but so is each source color's brightest
    /// channel, and the average is then rescaled to that brightness -- without
    /// the rescale, mixing two dyes could only ever darken. A color already on
    /// the item counts as one more dye, which is why re-dyeing leather drifts
    /// rather than replaces.
    ///
    /// Returns `None` when there is nothing at all to average; vanilla divides
    /// by the count unguarded and never reaches that case, because
    /// `SetRandomDyesFunction` returns early on a non-positive roll count.
    #[must_use]
    pub fn apply_dyes(current: Option<Self>, dyes: &[crate::DyeColor]) -> Option<Self> {
        let mut red_total = 0;
        let mut green_total = 0;
        let mut blue_total = 0;
        let mut intensity_total = 0;
        let mut color_count = 0;

        let sources = current
            .map(Self::rgb)
            .into_iter()
            .chain(dyes.iter().map(|dye| dye.texture_diffuse_color()));
        for color in sources {
            let red = (color >> 16) & 0xFF;
            let green = (color >> 8) & 0xFF;
            let blue = color & 0xFF;
            intensity_total += red.max(green).max(blue);
            red_total += red;
            green_total += green;
            blue_total += blue;
            color_count += 1;
        }

        if color_count == 0 {
            return None;
        }

        let red = red_total / color_count;
        let green = green_total / color_count;
        let blue = blue_total / color_count;
        let average_intensity = intensity_total as f32 / color_count as f32;
        let result_intensity = red.max(green).max(blue) as f32;
        let red = (red as f32 * average_intensity / result_intensity) as i32;
        let green = (green as f32 * average_intensity / result_intensity) as i32;
        let blue = (blue as f32 * average_intensity / result_intensity) as i32;

        // Vanilla parity: `ARGB.color(0, ..)`, which leaves the alpha byte zero.
        Some(Self::new(
            ((red & 0xFF) << 16) | ((green & 0xFF) << 8) | (blue & 0xFF),
        ))
    }
}

impl WriteTo for DyedItemColor {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        self.rgb.write(writer)
    }
}

impl ReadFrom for DyedItemColor {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        Ok(Self::new(i32::read(data)?))
    }
}

impl ToNbtTag for DyedItemColor {
    fn to_nbt_tag(self) -> NbtTag {
        NbtTag::Int(self.rgb)
    }
}

impl FromNbtTag for DyedItemColor {
    fn from_nbt_tag(tag: simdnbt::borrow::NbtTag<'_, '_>) -> Option<Self> {
        decode_rgb_color(&tag.to_owned()).map(Self::new)
    }
}

impl HashComponent for DyedItemColor {
    fn hash_component(&self, hasher: &mut ComponentHasher) {
        hasher.put_int(self.rgb);
    }
}

/// Color used to tint a filled map item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapItemColor {
    rgb: i32,
}

impl MapItemColor {
    pub const DEFAULT: Self = Self::new(4_603_950);

    #[must_use]
    pub const fn new(rgb: i32) -> Self {
        Self { rgb }
    }

    #[must_use]
    pub const fn rgb(self) -> i32 {
        self.rgb
    }
}

impl WriteTo for MapItemColor {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        self.rgb.write(writer)
    }
}

impl ReadFrom for MapItemColor {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        Ok(Self::new(i32::read(data)?))
    }
}

impl ToNbtTag for MapItemColor {
    fn to_nbt_tag(self) -> NbtTag {
        NbtTag::Int(self.rgb)
    }
}

impl FromNbtTag for MapItemColor {
    fn from_nbt_tag(tag: simdnbt::borrow::NbtTag<'_, '_>) -> Option<Self> {
        tag.codec_i32().map(Self::new)
    }
}

impl HashComponent for MapItemColor {
    fn hash_component(&self, hasher: &mut ComponentHasher) {
        hasher.put_int(self.rgb);
    }
}

/// Numeric identifier for a map saved-data entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapId {
    id: i32,
}

impl MapId {
    #[must_use]
    pub const fn new(id: i32) -> Self {
        Self { id }
    }

    #[must_use]
    pub const fn id(self) -> i32 {
        self.id
    }

    #[must_use]
    pub fn key(self) -> String {
        format!("maps/{}", self.id)
    }
}

impl WriteTo for MapId {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        VarInt(self.id).write(writer)
    }
}

impl ReadFrom for MapId {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        Ok(Self::new(VarInt::read(data)?.0))
    }
}

impl ToNbtTag for MapId {
    fn to_nbt_tag(self) -> NbtTag {
        NbtTag::Int(self.id)
    }
}

impl FromNbtTag for MapId {
    fn from_nbt_tag(tag: simdnbt::borrow::NbtTag<'_, '_>) -> Option<Self> {
        tag.codec_i32().map(Self::new)
    }
}

impl HashComponent for MapId {
    fn hash_component(&self, hasher: &mut ComponentHasher) {
        hasher.put_int(self.id);
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::ToNbtTag as _;
    use simdnbt::borrow::read_tag;
    use simdnbt::owned::{NbtList, NbtTag};
    use steel_utils::hash::HashComponent as _;
    use steel_utils::serial::{ReadFrom as _, WriteTo as _};

    use super::{DyedItemColor, MapId, MapItemColor};
    use crate::DyeColor;

    fn channels(color: i32) -> (i32, i32, i32) {
        ((color >> 16) & 0xFF, (color >> 8) & 0xFF, color & 0xFF)
    }

    fn brightest(color: i32) -> i32 {
        let (red, green, blue) = channels(color);
        red.max(green).max(blue)
    }

    /// Mixing dyes averages the channels *and* the brightest channel of each
    /// source, then rescales the average back up to that brightness. Without the
    /// rescale, mixing could only ever darken -- two bright dyes of different
    /// hues would average into mud, and a leatherworker's dyed armor would come
    /// out progressively greyer the more dyes went in.
    #[test]
    fn mixing_two_dyes_keeps_the_brightness_they_averaged_to() {
        let red = DyeColor::Red.texture_diffuse_color();
        let white = DyeColor::White.texture_diffuse_color();

        let mixed = DyedItemColor::apply_dyes(None, &[DyeColor::Red, DyeColor::White])
            .expect("two dyes are something to average");

        let expected_brightness = i32::midpoint(brightest(red), brightest(white));
        assert_eq!(
            brightest(mixed.rgb()),
            expected_brightness,
            "the mix should be as bright as its sources were on average"
        );

        // The plain channel average, which is what a mix without the rescale
        // would produce, is dimmer than that -- so this really is the rescale
        // being observed and not an accident of these two colors.
        let (red_r, red_g, red_b) = channels(red);
        let (white_r, white_g, white_b) = channels(white);
        let flat = i32::midpoint(red_r, white_r)
            .max(i32::midpoint(red_g, white_g))
            .max(i32::midpoint(red_b, white_b));
        assert!(
            flat < expected_brightness,
            "an unrescaled average would be dimmer, so the test can tell them apart"
        );
    }

    /// A color already on the item counts as one more dye rather than being
    /// replaced, which is what makes re-dyeing leather drift towards a mix.
    #[test]
    fn an_existing_color_is_mixed_in_rather_than_overwritten() {
        let only_white = DyedItemColor::apply_dyes(None, &[DyeColor::White])
            .expect("one dye is something to average");
        let over_red =
            DyedItemColor::apply_dyes(Some(DyedItemColor::new(0x00FF_0000)), &[DyeColor::White])
                .expect("a current color plus one dye is something to average");

        assert_ne!(
            over_red.rgb(),
            only_white.rgb(),
            "dyeing a red item white should not simply make it white"
        );
        let (red, _, blue) = channels(over_red.rgb());
        assert!(
            red > blue,
            "the red it started from should still dominate the mix"
        );
    }

    fn parse<T: simdnbt::FromNbtTag>(tag: NbtTag) -> Option<T> {
        let mut bytes = Vec::new();
        tag.write(&mut bytes);
        let borrowed = read_tag(&mut Cursor::new(bytes.as_slice())).ok()?;
        T::from_nbt_tag(borrowed.as_tag())
    }

    #[test]
    fn dyed_color_accepts_ints_and_rgb_vectors() {
        assert_eq!(
            parse(NbtTag::Short(0x1234)),
            Some(DyedItemColor::new(0x1234))
        );
        assert_eq!(
            parse(NbtTag::List(NbtList::Float(vec![1.0, 0.5, 0.0]))),
            Some(DyedItemColor::new(0xffff_7f00_u32 as i32))
        );
        assert_eq!(
            parse::<DyedItemColor>(NbtTag::List(NbtList::Float(vec![1.0]))),
            None
        );
        assert_eq!(
            DyedItemColor::new(0x123456).to_nbt_tag(),
            NbtTag::Int(0x123456)
        );
    }

    #[test]
    fn raw_int_network_codecs_round_trip() {
        for value in [DyedItemColor::new(-1), DyedItemColor::new(0x123456)] {
            let mut encoded = Vec::new();
            value.write(&mut encoded).expect("color should encode");
            assert_eq!(
                DyedItemColor::read(&mut Cursor::new(encoded.as_slice()))
                    .expect("color should decode"),
                value
            );
        }

        let value = MapItemColor::DEFAULT;
        let mut encoded = Vec::new();
        value.write(&mut encoded).expect("map color should encode");
        assert_eq!(
            MapItemColor::read(&mut Cursor::new(encoded.as_slice()))
                .expect("map color should decode"),
            value
        );
    }

    #[test]
    fn map_id_uses_varint_network_and_int_persistence() {
        let value = MapId::new(-17);
        let mut encoded = Vec::new();
        value.write(&mut encoded).expect("map ID should encode");
        assert_eq!(
            MapId::read(&mut Cursor::new(encoded.as_slice())).expect("map ID should decode"),
            value
        );
        assert_eq!(parse(NbtTag::Long(42)), Some(MapId::new(42)));
        assert_eq!(value.to_nbt_tag(), NbtTag::Int(-17));
        assert_eq!(MapId::new(42).key(), "maps/42");
    }

    #[test]
    fn persistent_hashes_use_int_codec_shape() {
        for (actual, expected) in [
            (
                DyedItemColor::new(0x123456).compute_hash(),
                NbtTag::Int(0x123456).compute_hash(),
            ),
            (
                MapItemColor::DEFAULT.compute_hash(),
                NbtTag::Int(MapItemColor::DEFAULT.rgb()).compute_hash(),
            ),
            (MapId::new(7).compute_hash(), NbtTag::Int(7).compute_hash()),
        ] {
            assert_eq!(actual, expected);
        }
    }
}
