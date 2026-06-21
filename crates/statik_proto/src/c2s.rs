pub mod handshake;
pub mod login;
pub mod play;
pub mod status;

use handshake::*;
use login::*;
use play::*;
use statik_derive::PacketGroup;
use status::*;

#[derive(Debug, PacketGroup)]
pub enum C2SPacket {
    //Handshake
    Handshake(C2SHandshake),

    //Status
    StatusRequest(C2SStatusRequest),
    Ping(C2SPing),

    //Login
    LoginStart(C2SLoginStart),
    EncryptionResponse(C2SEncryptionResponse),
    LoginPluginResponse(C2SLoginPluginResponse),

    //Play
    AcceptTeleportation(C2SAcceptTeleportation),
    ClientInformation(C2SClientInformation),
    KeepAlive(C2SKeepAlive),
    PlayerPos(C2SPlayerPos),
    PlayerPosRot(C2SPlayerPosRot),
    PlayerRot(C2SPlayerRot),
}
