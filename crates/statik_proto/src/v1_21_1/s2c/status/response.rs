//! The JSON body of `S2CStatusResponse`. The version name / protocol
//! number are supplied at construction time (not from compile-time
//! constants), so the same struct serves every supported MC version.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use statik_core::prelude::*;
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusResponse {
    version: Version,
    players: Players,
    description: Chat,
    #[serde(skip_serializing_if = "Option::is_none")]
    favicon: Option<String>,
    enforces_secure_chat: bool,
}

impl std::fmt::Debug for StatusResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Formatter::debug_struct(f, "StatusResponse")
            .field("version", &self.version)
            .field("players", &self.players)
            .field("description", &self.description)
            .field(
                "favicon",
                match &self.favicon {
                    Some(_) => &"#HIDDEN: ICON AS BASE64 STRING#",
                    None => &None::<String>,
                },
            )
            .field("enforces_secure_chat", &self.enforces_secure_chat)
            .finish()
    }
}

impl StatusResponse {
    pub fn new(
        version_name: impl Into<Cow<'static, str>>,
        protocol: usize,
        players: Players,
        description: Chat,
        favicon: Option<String>,
        enforces_secure_chat: bool,
    ) -> Self {
        Self {
            version: Version::new(version_name, protocol),
            players,
            description,
            favicon: favicon.map(|data| format!("data:image/png;base64,{data}")),
            enforces_secure_chat,
        }
    }
}

impl Encode for StatusResponse {
    fn encode(&self, buffer: impl std::io::Write) -> Result<()> {
        serde_json::to_string(self)?.encode(buffer)
    }
}

impl Decode for StatusResponse {
    fn decode(buffer: impl std::io::Read) -> Result<Self> {
        Ok(serde_json::from_str(&String::decode(buffer)?)?)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Version {
    name: Cow<'static, str>,
    protocol: usize,
}

impl Version {
    pub fn new<S: Into<Cow<'static, str>>>(name: S, protocol: usize) -> Self {
        Self {
            name: name.into(),
            protocol,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Players {
    max: i32,
    online: i32,
    sample: Vec<PlayerSample>,
}

impl Players {
    pub fn new(max: i32, online: i32, sample: Vec<PlayerSample>) -> Self {
        Self {
            max,
            online,
            sample,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlayerSample {
    name: Cow<'static, str>,
    id: Uuid,
}

impl PlayerSample {
    pub fn new<S: Into<Cow<'static, str>>>(name: S, id: Uuid) -> Self {
        Self {
            name: name.into(),
            id,
        }
    }
}
