use foton_macros::{ClientPacket, WriteTo};
use foton_registry::packets::status::C_STATUS_RESPONSE;
use serde::Serialize;
use text_components::TextComponent;

/// Writes a component in Minecraft's JSON text form, hex colours included.
fn minecraft_json<S: serde::Serializer>(
    component: &TextComponent,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    foton_utils::text::json::to_json(component).serialize(serializer)
}

#[derive(Serialize, Clone, Debug)]
pub struct Sample {
    /// The player's name.
    pub name: String,
    /// The player's UUID.
    pub id: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Players {
    pub max: i32,
    pub online: i32,
    pub sample: Vec<Sample>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Version {
    pub name: &'static str,
    pub protocol: i32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// The MOTD, as a text component so it may carry colours.
    #[serde(serialize_with = "minecraft_json")]
    pub description: TextComponent,
    pub players: Option<Players>,
    pub version: Option<Version>,
    pub favicon: Option<String>,
    pub enforces_secure_chat: bool,
}

#[derive(ClientPacket, WriteTo, Clone, Debug)]
#[packet_id(Status = C_STATUS_RESPONSE)]
pub struct CStatusResponse {
    #[write(as = Json)]
    status: Status,
}

impl CStatusResponse {
    #[must_use]
    pub const fn new(status: Status) -> Self {
        Self { status }
    }
}

#[cfg(test)]
mod tests {
    use super::Status;

    #[test]
    fn secure_chat_enforcement_uses_vanilla_json_name() {
        let status = Status {
            description: text_components::TextComponent::new(),
            players: None,
            version: None,
            favicon: None,
            enforces_secure_chat: true,
        };
        let json = serde_json::to_value(status).expect("status should serialize");

        assert_eq!(
            json.get("enforcesSecureChat"),
            Some(&serde_json::json!(true))
        );
        assert!(json.get("enforce_secure_chat").is_none());
    }
}
