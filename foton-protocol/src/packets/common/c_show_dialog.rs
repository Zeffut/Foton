use std::io::{Result, Write};

use foton_macros::{ClientPacket, WriteTo};
use foton_registry::packets::config::C_SHOW_DIALOG;
use foton_registry::packets::play::C_SHOW_DIALOG as PLAY_C_SHOW_DIALOG;
use foton_utils::serial::WriteTo;
use simdnbt::owned::NbtCompound;

/// Opens a server-built dialog on the client.
///
/// Vanilla parity: `ClientboundShowDialogPacket`, whose payload is a
/// `Holder<Dialog>`. A holder is a var-int discriminant followed by either
/// nothing (a registry reference, written as id + 1) or the value itself
/// (written as 0). Foton only ever sends the inline form: these dialogs are
/// built for one player at one moment and have no business in the registry.
#[derive(ClientPacket, WriteTo, Clone, Debug)]
#[packet_id(Config = C_SHOW_DIALOG, Play = PLAY_C_SHOW_DIALOG)]
pub struct CShowDialog {
    /// The holder discriminant. Always zero -- see the type comment.
    #[write(as = VarInt)]
    pub inline: i32,
    /// The dialog, in the shape `Dialog.CODEC` reads.
    pub dialog: DialogBody,
}

/// A dialog compound, written as the network NBT the client expects.
///
/// The wrapper is not decoration: `simdnbt`'s own inherent `write` shadows the
/// `WriteTo` trait method at a call site, so a bare `NbtCompound` field makes
/// the derive resolve to the wrong one.
#[derive(Clone, Debug)]
pub struct DialogBody(pub NbtCompound);

impl WriteTo for DialogBody {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        // The leading type byte is not optional. `NbtCompound::write` emits the
        // compound's *contents*; the wire carries a full tag, so the reader
        // expects `TAG_Compound` in front of them. Without it every byte after
        // this point is read one position early, the packet stops making sense,
        // and the client dies rather than reporting anything -- which is what
        // shipping this without the byte actually did.
        writer.write_all(&[TAG_COMPOUND])?;
        let mut buf = Vec::new();
        NbtCompound::write(&self.0, &mut buf);
        writer.write_all(&buf)
    }
}

/// The NBT tag id of a compound, as the network form writes it.
const TAG_COMPOUND: u8 = 0x0A;

impl CShowDialog {
    /// Sends `dialog` as an inline holder.
    #[must_use]
    pub const fn inline(dialog: NbtCompound) -> Self {
        Self {
            inline: 0,
            dialog: DialogBody(dialog),
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    /// The encoded packet is a holder discriminant then a whole NBT tag.
    ///
    /// This is a byte-level test on purpose. The failure it guards has no
    /// server-side symptom at all: the packet is built correctly, sent
    /// correctly, and logged as sent, and the only thing that happens is that
    /// the player's client dies while decoding it.
    #[test]
    fn the_dialog_goes_out_as_an_inline_holder_and_a_tagged_compound() {
        let mut compound = NbtCompound::new();
        compound.insert("type", "minecraft:notice");

        let mut bytes = Vec::new();
        CShowDialog::inline(compound)
            .write(&mut bytes)
            .expect("writing to a vec cannot fail");

        assert_eq!(
            bytes.first(),
            Some(&0),
            "a var-int zero marks the holder's inline half"
        );
        assert_eq!(
            bytes.get(1),
            Some(&TAG_COMPOUND),
            "the dialog is a whole NBT tag, so its type byte comes first"
        );
    }
}
