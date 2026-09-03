//! Clientbound title packets.
//!
//! These mirror Minecraft's four title protocol messages used by Bukkit's
//! `Player.sendTitle` and Paper's `Title` API.

use foton_macros::{ClientPacket, WriteTo};
use foton_registry::packets::play::{
    C_CLEAR_TITLES, C_SET_SUBTITLE_TEXT, C_SET_TITLE_TEXT, C_SET_TITLES_ANIMATION,
};
use text_components::{TextComponent, resolving::TextResolutor};

#[derive(ClientPacket, WriteTo, Clone, Debug)]
#[packet_id(Play = C_SET_TITLE_TEXT)]
pub struct CSetTitleText {
    pub text: TextComponent,
}

impl CSetTitleText {
    pub fn new<T: TextResolutor>(text: &TextComponent, player: &T) -> Self {
        Self {
            text: text.resolve(player),
        }
    }
}

#[derive(ClientPacket, WriteTo, Clone, Debug)]
#[packet_id(Play = C_SET_SUBTITLE_TEXT)]
pub struct CSetSubtitleText {
    pub text: TextComponent,
}

impl CSetSubtitleText {
    pub fn new<T: TextResolutor>(text: &TextComponent, player: &T) -> Self {
        Self {
            text: text.resolve(player),
        }
    }
}

#[derive(ClientPacket, WriteTo, Clone, Copy, Debug)]
#[packet_id(Play = C_SET_TITLES_ANIMATION)]
pub struct CSetTitlesAnimation {
    pub fade_in: i32,
    pub stay: i32,
    pub fade_out: i32,
}

#[derive(ClientPacket, WriteTo, Clone, Copy, Debug)]
#[packet_id(Play = C_CLEAR_TITLES)]
pub struct CClearTitles {
    pub reset_times: bool,
}
