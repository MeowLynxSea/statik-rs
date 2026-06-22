pub mod configuration;
pub mod login;
pub mod play;
pub mod status;

use configuration::*;
use login::*;
use play::*;
use statik_derive::PacketGroup;
use status::*;

/// Aggregated S2C packets for protocol 767 (MC 1.21.1).
#[derive(Debug, PacketGroup)]
pub enum S2CPacket {
    // Status
    StatusResponse(S2CStatusResponse),
    Pong(S2CPong),

    // Login
    Disconnect(S2CDisconnect),
    EncryptionRequest(S2CEncryptionRequest),
    LoginSuccess(S2CLoginSuccess),
    SetCompression(S2CSetCompression),
    LoginPluginRequest(S2CLoginPluginRequest),

    // Configuration
    CustomPayload(S2CCustomPayload),
    DisconnectConfiguration(S2CDisconnectConfiguration),
    FinishConfiguration(S2CFinishConfiguration),
    ConfigurationKeepAlive(S2CConfigurationKeepAlive),
    PingConfiguration(S2CPingConfiguration),
    RegistryData(S2CRegistryData),
    FeatureFlags(S2CFeatureFlags),
    KnownPacks(S2CKnownPacks),

    // Play (limbo only)
    DisconnectPlay(S2CDisconnectPlay),
    GameEvent(S2CGameEvent),
    KeepAlive(S2CKeepAlive),
    LevelChunkWithLight(S2CLevelChunkWithLight),
    Login(S2CLogin),
    PlayerAbilities(S2CPlayerAbilities),
    PlayerPosition(S2CPlayerPosition),
    SetChunkCacheCenter(S2CSetChunkCacheCenter),
    SetChunkCacheRadius(S2CSetChunkCacheRadius),
    SetDefaultSpawnPosition(S2CSetDefaultSpawnPosition),
}
