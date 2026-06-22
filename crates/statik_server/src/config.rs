use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub general: GeneralServerConfig,
    pub mc: McServerConfig,
    pub api: ApiServerConfig,
    pub limbo: LimboConfig,
    pub compression: CompressionConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GeneralServerConfig {
    /// Defaults to "0.0.0.0" which accepts all incoming connections.
    pub host: String,
    pub log_level: String,
}

impl Default for GeneralServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            log_level: "debug".to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct McServerConfig {
    /// The port minecraft clients connect to. Defaults to 25565.
    pub port: u16,

    /// Target Minecraft version to serve. Accepts a version string
    /// (`"1.20.1"`, `"1.21.1"`) or a protocol number (`"763"`, `"767"`).
    /// Defaults to `"1.20.1"`.
    pub version: Option<String>,

    /// How many players can join the server at once. Defaults to 20.
    ///
    /// Note: negative values are valid for setting the max number of
    /// players on a minecraft server.
    pub max_players: i32,

    /// Hides the player count to a querying client. Defaults to false.
    ///
    /// Note: for continuity, this should probably match the setting of
    /// your actual minecraft server.
    pub hide_player_count: bool,

    /// The "Message of the Day" (text displayed when a client checks
    /// the status of a server). Can be formatted with the § symbol
    /// as defined on the [Minecraft wiki page](https://minecraft.fandom.com/wiki/Formatting_codes).
    ///
    /// Defaults to "A Statik server!"
    ///
    /// Note: for continuity, this should probably match the MOTD of
    /// your actual minecraft server.
    pub motd: String,

    /// The maximum size (in bytes) that a packet can be.
    /// Defaults to 4096.
    pub max_packet_size: usize,

    /// The URI (unique reference identifier) corresponding to the ~~website
    /// link or~~ local file containing the server icon.
    ///
    /// Note: must be a 64x64 pixel image, or the minecraft client will not
    /// be able to parse it, and the server will have a blank icon.
    pub icon: Option<String>,

    /// Whether this server appears online or not. Defaults to false.
    ///
    /// Note: this would pretty much make the statik server worthless!
    /// Make sure you are certain this is what you want to enable.
    pub hidden: bool,

    /// What message should be sent to the client by default when disconnecting
    /// them. Defaults to: "Disconnected from the server."
    ///
    /// Note: this can be overridden by disconnect specific packets, this is
    /// merely the default, no reason given fallback message.
    /// Note: can be templated using [Tera](https://tera.netlify.app/), a templating
    /// library inspired by Jinja2 and Django - read their [Documentation](https://tera.netlify.app/docs/)
    /// and [Examples](https://github.com/Keats/tera/tree/master/examples) for possible
    /// templates.
    pub disconnect_msg: String,
}

impl Default for McServerConfig {
    fn default() -> Self {
        Self {
            max_packet_size: 4096,
            port: 25565,
            version: None,
            max_players: 20,
            hide_player_count: false,
            motd: "A Statik server!".to_string(),
            icon: None,
            hidden: false,
            disconnect_msg: "Disconnected from the server.".to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ApiServerConfig {
    /// The port api connections use. Defaults to 8080.
    pub port: usize,

    /// Shared secret used to authenticate api requests. If `None`, the api
    /// accepts unauthenticated `ping`/`status` requests but refuses
    /// `shutdown` (and other mutating actions) for safety. If set, every
    /// request must supply a matching `token` field.
    ///
    /// This is intended for a local supervisor process (e.g. one that
    /// triggers the real minecraft server to start) and is not a substitute
    /// for transport security — keep the api listener on localhost.
    pub token: Option<String>,
}

impl Default for ApiServerConfig {
    fn default() -> Self {
        Self {
            port: 8080,
            token: None,
        }
    }
}

/// Configuration for the limbo world clients are placed into after login.
///
/// Limbo is unconditional: every successful `LoginStart` transitions the
/// client into Play state at the configured position in an empty void world.
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct LimboConfig {
    /// Fixed spawn / locked position (block coordinates, x/y/z). Players can
    /// look around but `isFlying=true` and the single `Synchronize Player
    /// Position` packet sent on login prevent them from leaving this point.
    pub position: [f64; 3],

    /// Game mode: 0 = survival, 1 = creative, 2 = adventure, 3 = spectator.
    /// Sent as the `gameType` field of the Login packet.
    pub gamemode: i32,

    /// Chunk view distance in chunks (radius). Sent via Set Chunk Cache
    /// Radius (0x4F). 8 = reasonable default; max 32 in vanilla.
    pub view_distance: i32,

    /// Simulation distance in chunks. Sent as a field of the Login packet
    /// (0x28). Must be ≤ `view_distance`.
    pub simulation_distance: i32,

    /// Dimension name, e.g. `"minecraft:the_void"`. Used as both the
    /// `dimension` and `dimensionType` fields of the Login packet.
    pub dimension: String,
}

impl Default for LimboConfig {
    fn default() -> Self {
        Self {
            position: [0.5, 64.0, 0.5],
            gamemode: 1, // creative
            view_distance: 8,
            simulation_distance: 8,
            dimension: "minecraft:the_void".to_string(),
        }
    }
}

/// Configuration for Minecraft packet compression.
///
/// When `enabled = true`, the server sends `S2CSetCompression` (Login 0x03)
/// before `LoginSuccess` and compresses all subsequent packets whose body
/// length meets `threshold`. When `enabled = false` (the default), packets
/// are sent uncompressed, matching the original statik behavior.
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct CompressionConfig {
    /// Whether to enable zlib compression for Minecraft packets.
    pub enabled: bool,

    /// Compression threshold in bytes. Packets with body length below this
    /// value are sent uncompressed (with a leading `VarInt(0)` Data Length).
    /// Vanilla default is 256.
    pub threshold: i32,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold: 256,
        }
    }
}
