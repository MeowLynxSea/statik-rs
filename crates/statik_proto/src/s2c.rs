pub mod login;
pub mod play;
pub mod status;

use login::*;
use play::*;
use statik_derive::PacketGroup;
use status::*;

#[derive(Debug, PacketGroup)]
pub enum S2CPacket {
    //Status
    StatusResponse(S2CStatusResponse),
    Pong(S2CPong),

    //Login
    Disconnect(S2CDisconnect),
    EncryptionRequest(S2CEncryptionRequest),
    LoginSuccess(S2CLoginSuccess),
    SetCompression(S2CSetCompression),
    LoginPluginRequest(S2CLoginPluginRequest),

    //Play
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
