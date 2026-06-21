//! Round-trip tests for the proc-macro-derived packet system.
//!
//! Each packet is encoded via its `Packet`-derived `Encode` impl (which writes
//! the leading VarInt id followed by its fields) and then decoded via the
//! `PacketGroup`-derived `decode_in_state`, disambiguating by protocol state.
//!
//! Minecraft reuses packet ids across states (e.g. 0x00 exists in Handshake,
//! Status and Login for C2S), so decoding by id alone is ambiguous. These
//! tests pin the state-aware dispatch behaviour.

use statik_core::prelude::*;
use statik_proto::prelude::*;
use uuid::Uuid;

fn encode<P: Packet>(packet: &P) -> Vec<u8> {
    let mut buf = Vec::new();
    packet.encode(&mut buf).expect("encode");
    buf
}

#[test]
fn handshake_roundtrip() {
    let pkt = C2SHandshake {
        protocol_version: VarInt(763),
        server_address: "example.com".to_string(),
        server_port: 25565,
        next_state: State::Status,
    };
    let buf = encode(&pkt);

    let decoded = C2SPacket::decode_in_state(State::Handshake, &mut &buf[..]).expect("decode");
    match decoded {
        C2SPacket::Handshake(h) => {
            assert_eq!(h.protocol_version.0, 763);
            assert_eq!(h.server_address, "example.com");
            assert_eq!(h.server_port, 25565);
            assert_eq!(h.next_state, State::Status);
        }
        other => panic!("expected Handshake, got {other:?}"),
    }
}

#[test]
fn status_request_roundtrip() {
    // id 0x00, no fields — collides with C2SHandshake/C2SLoginStart on id
    // alone, so state-aware dispatch is required.
    let pkt = C2SStatusRequest {};
    let buf = encode(&pkt);
    assert_eq!(buf, [0x00]);

    let decoded = C2SPacket::decode_in_state(State::Status, &mut &buf[..]).expect("decode");
    assert!(matches!(decoded, C2SPacket::StatusRequest(_)));
}

#[test]
fn ping_roundtrip() {
    // Use a positive value well within i64::MAX so the round-trip is valid
    // against a signed wire format. The Minecraft client picks an arbitrary
    // value; the exact magnitude is not significant.
    let pkt = C2SPing {
        payload: 0x0123_4567_89ab_cdef_i64,
    };
    let buf = encode(&pkt);

    let decoded = C2SPacket::decode_in_state(State::Status, &mut &buf[..]).expect("decode");
    match decoded {
        C2SPacket::Ping(p) => assert_eq!(p.payload, pkt.payload),
        other => panic!("expected Ping, got {other:?}"),
    }
}

#[test]
fn login_start_roundtrip() {
    // id 0x00 in Login — collides with Handshake/StatusRequest on id alone.
    let pkt = C2SLoginStart {
        username: "Player1".to_string(),
        uuid: Some(Uuid::nil()),
    };
    let buf = encode(&pkt);

    let decoded = C2SPacket::decode_in_state(State::Login, &mut &buf[..]).expect("decode");
    match decoded {
        C2SPacket::LoginStart(s) => {
            assert_eq!(s.username, "Player1");
            assert_eq!(s.uuid, Some(Uuid::nil()));
        }
        other => panic!("expected LoginStart, got {other:?}"),
    }
}

#[test]
fn s2c_disconnect_roundtrip() {
    // id 0x00 in Login — collides with S2CStatusResponse on id alone.
    let pkt = S2CDisconnect {
        reason: Chat::new("bye"),
    };
    let buf = encode(&pkt);

    let decoded = S2CPacket::decode_in_state(State::Login, &mut &buf[..]).expect("decode");
    assert!(matches!(decoded, S2CPacket::Disconnect(_)));
}

#[test]
fn wrong_state_is_rejected() {
    // C2SPing has id 0x01 in Status. The Handshake state has no packet with
    // id 0x01 (only C2SHandshake at 0x00), so decoding it there must error.
    let pkt = C2SPing { payload: 42 };
    let buf = encode(&pkt);

    let result = C2SPacket::decode_in_state(State::Handshake, &mut &buf[..]);
    assert!(
        result.is_err(),
        "expected decode to reject id 0x01 in Handshake"
    );
}
