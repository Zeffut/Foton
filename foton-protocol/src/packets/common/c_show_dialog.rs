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
        let mut buf = Vec::new();
        NbtCompound::write(&self.0, &mut buf);
        writer.write_all(&buf)
    }
}

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
