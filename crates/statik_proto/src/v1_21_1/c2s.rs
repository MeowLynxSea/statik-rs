pub mod configuration;
pub mod handshake;
pub mod login;
pub mod play;
pub mod status;

use configuration::*;
use handshake::*;
use login::*;
use play::*;
use statik_derive::PacketGroup;
use status::*;

/// Aggregated C2S packets for protocol 767 (MC 1.21.1).
///
/// Decoded by `Connection` via [`C2SPacket::decode_in_state`] when the
/// connection is in 1.21.1 mode. The set of variants differs from 1.20.1 —
/// most notably the new Configuration state (entered after `Login
/// Acknowledged`) and the renamed `Hello` (Login 0x00) / `flying` (Play
/// 0x17) packets.
#[derive(Debug, PacketGroup)]
pub enum C2SPacket {
    // Handshake
    Handshake(C2SHandshake),

    // Status
    StatusRequest(C2SStatusRequest),
    Ping(C2SPing),

    // Login
    Hello(C2SHello),
    Key(C2SKey),
    CustomQueryAnswer(C2SCustomQueryAnswer),
    LoginAcknowledged(C2SLoginAcknowledged),
    CookieResponse(C2SLoginCookieResponse),

    // Configuration
    ClientInformation(C2SClientInformation),
    ConfigurationCookieResponse(C2SCookieResponse),
    ConfigurationCustomPayload(C2SConfigurationCustomPayload),
    FinishConfigurationAck(C2SFinishConfiguration),
    ConfigurationKeepAlive(C2SConfigurationKeepAlive),
    PongConfiguration(C2SPongConfiguration),
    ResourcePackResponse(C2SResourcePackResponse),
    SelectKnownPacks(C2SSelectKnownPacks),

    // Play (limbo only)
    AcceptTeleportation(C2SAcceptTeleportation),
    KeepAlive(C2SKeepAlive),
    PlayerPos(C2SPlayerPos),
    PlayerPosRot(C2SPlayerPosRot),
    PlayerRot(C2SPlayerRot),
    PlayerFlying(C2SPlayerFlying),
}
