use statik_core::prelude::*;
use statik_derive::*;

#[derive(Debug, Packet)]
#[packet(id = 0x00, state = State::Handshake)]
pub struct C2SHandshake {
    ///See [protocol version numbers](https://wiki.vg/Protocol_version_numbers) (currently 763 in Minecraft 1.20.1).
    pub protocol_version: VarInt,
    ///Hostname or IP, e.g. localhost or 127.0.0.1, that was used to connect.
    /// The Notchian server does not use this information. Note that SRV records
    /// are a simple redirect, e.g. if _minecraft._tcp.example.com points to
    /// mc.example.org, users connecting to example.com will provide example.org
    /// as server address in addition to connecting to it.
    pub server_address: String,
    ///Default is 25565. The Notchian server does not use this information.
    ///
    /// Wire format is an unsigned short (2 bytes, big-endian). Mojang stores
    /// this in a Java `int` field but serialises it with `readUnsignedShort`.
    /// Some protocol dumps (e.g. the bundled readmes) list the field as
    /// `int`; that column reflects the Java type, not the on-wire width.
    pub server_port: u16,
    ///1 for Status, 2 for Login, 3 for Transfer (1.21+).
    ///
    /// This is [`statik_core::handshake::ClientIntent`], **not** [`State`]: it
    /// is decoupled from `State` so that `State::Configuration = 3` does not
    /// collide with the Transfer handshake value.
    pub next_state: ClientIntent,
}
