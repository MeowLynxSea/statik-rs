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
use statik_proto::{
    common::abilities,
    v1_20_1::{
        c2s::{
            handshake::C2SHandshake,
            login::C2SLoginStart,
            play::{
                C2SAcceptTeleportation, C2SClientInformation, C2SCustomPayload, C2SKeepAlive,
                C2SPlayerPos, C2SPlayerPosRot, C2SPlayerRot,
            },
            status::{C2SPing, C2SStatusRequest},
            C2SPacket,
        },
        s2c::{
            login::S2CDisconnect,
            play::{S2CGameEvent, S2CPlayerAbilities, S2CPlayerPosition},
            S2CPacket,
        },
    },
};
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
        next_state: ClientIntent::Status,
    };
    let buf = encode(&pkt);

    let decoded = C2SPacket::decode_in_state(State::Handshake, &mut &buf[..]).expect("decode");
    match decoded {
        C2SPacket::Handshake(h) => {
            assert_eq!(h.protocol_version.0, 763);
            assert_eq!(h.server_address, "example.com");
            assert_eq!(h.server_port, 25565);
            assert_eq!(h.next_state, ClientIntent::Status);
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

// == Play state roundtrip tests == \\

#[test]
fn play_accept_teleportation_roundtrip() {
    let pkt = C2SAcceptTeleportation { id: VarInt(0) };
    let buf = encode(&pkt);

    let decoded = C2SPacket::decode_in_state(State::Play, &mut &buf[..]).expect("decode");
    match decoded {
        C2SPacket::AcceptTeleportation(t) => assert_eq!(t.id, VarInt(0)),
        other => panic!("expected AcceptTeleportation, got {other:?}"),
    }
}

#[test]
fn play_keepalive_roundtrip() {
    // id 0x12 in Play — collides with Status state Ping? No, ping is in
    // Status state. In Play state id 0x12 is the keepalive response.
    let pkt = C2SKeepAlive {
        id: 0x0123_4567_89ab_cdef_i64,
    };
    let buf = encode(&pkt);

    let decoded = C2SPacket::decode_in_state(State::Play, &mut &buf[..]).expect("decode");
    match decoded {
        C2SPacket::KeepAlive(k) => assert_eq!(k.id, pkt.id),
        other => panic!("expected KeepAlive, got {other:?}"),
    }
}

#[test]
fn play_player_pos_roundtrip() {
    let pkt = C2SPlayerPos {
        x: 1.0,
        y: 64.5,
        z: -3.25,
        on_ground: true,
    };
    let buf = encode(&pkt);

    let decoded = C2SPacket::decode_in_state(State::Play, &mut &buf[..]).expect("decode");
    match decoded {
        C2SPacket::PlayerPos(p) => {
            assert_eq!(p.x, 1.0);
            assert_eq!(p.y, 64.5);
            assert_eq!(p.z, -3.25);
            assert!(p.on_ground);
        }
        other => panic!("expected PlayerPos, got {other:?}"),
    }
}

#[test]
fn s2c_player_abilities_roundtrip() {
    let pkt = S2CPlayerAbilities {
        flags: abilities::INVULNERABLE | abilities::FLYING | abilities::CAN_FLY,
        flying_speed: 0.05,
        walking_speed: 0.1,
    };
    let buf = encode(&pkt);

    let decoded = S2CPacket::decode_in_state(State::Play, &mut &buf[..]).expect("decode");
    match decoded {
        S2CPacket::PlayerAbilities(a) => {
            assert_eq!(
                a.flags,
                abilities::INVULNERABLE | abilities::FLYING | abilities::CAN_FLY
            );
            assert_eq!(a.flying_speed, 0.05);
            assert_eq!(a.walking_speed, 0.1);
        }
        other => panic!("expected PlayerAbilities, got {other:?}"),
    }
}

#[test]
fn s2c_player_position_roundtrip() {
    let pkt = S2CPlayerPosition {
        x: 0.5,
        y: 64.0,
        z: 0.5,
        y_rot: 0.0,
        x_rot: 0.0,
        relative_arguments: 0,
        id: VarInt(0),
    };
    let buf = encode(&pkt);

    let decoded = S2CPacket::decode_in_state(State::Play, &mut &buf[..]).expect("decode");
    match decoded {
        S2CPacket::PlayerPosition(p) => {
            assert_eq!(p.x, 0.5);
            assert_eq!(p.y, 64.0);
            assert_eq!(p.z, 0.5);
            assert_eq!(p.id, VarInt(0));
        }
        other => panic!("expected PlayerPosition, got {other:?}"),
    }
}

#[test]
fn s2c_game_event_roundtrip() {
    let pkt = S2CGameEvent {
        event: 7,
        param: 0.0,
    };
    let buf = encode(&pkt);

    let decoded = S2CPacket::decode_in_state(State::Play, &mut &buf[..]).expect("decode");
    match decoded {
        S2CPacket::GameEvent(g) => {
            assert_eq!(g.event, 7);
            assert_eq!(g.param, 0.0);
        }
        other => panic!("expected GameEvent, got {other:?}"),
    }
}

#[test]
fn c2s_client_information_roundtrip() {
    let pkt = C2SClientInformation {
        language: "zh_cn".to_string(),
        view_distance: 8,
        chat_visibility: VarInt(0),
        chat_colors: true,
        skin_parts: 127,
        main_hand: VarInt(1),
        text_filtering_enabled: true,
        allows_listing: true,
    };
    let buf = encode(&pkt);

    let decoded = C2SPacket::decode_in_state(State::Play, &mut &buf[..]).expect("decode");
    match decoded {
        C2SPacket::ClientInformation(c) => {
            assert_eq!(c.language, "zh_cn");
            assert_eq!(c.view_distance, 8);
            assert_eq!(c.chat_visibility, VarInt(0));
            assert!(c.chat_colors);
            assert_eq!(c.skin_parts, 127);
            assert_eq!(c.main_hand, VarInt(1));
            assert!(c.text_filtering_enabled);
            assert!(c.allows_listing);
        }
        other => panic!("expected ClientInformation, got {other:?}"),
    }
}

#[test]
fn c2s_custom_payload_roundtrip() {
    let pkt = C2SCustomPayload {
        channel: "minecraft:brand".to_string(),
        data: RawBytes::new(vec![0x07, b'v', b'a', b'n', b'i', b'l', b'l', b'a']),
    };
    let buf = encode(&pkt);

    let decoded = C2SPacket::decode_in_state(State::Play, &mut &buf[..]).expect("decode");
    match decoded {
        C2SPacket::CustomPayload(c) => {
            assert_eq!(c.channel, "minecraft:brand");
            assert_eq!(
                &*c.data.0,
                &[0x07, b'v', b'a', b'n', b'i', b'l', b'l', b'a']
            );
        }
        other => panic!("expected CustomPayload, got {other:?}"),
    }
}

#[test]
fn c2s_player_pos_rot_roundtrip() {
    let pkt = C2SPlayerPosRot {
        x: 1.0,
        y: 64.5,
        z: -3.25,
        y_rot: 180.0,
        x_rot: 45.0,
        on_ground: true,
    };
    let buf = encode(&pkt);

    let decoded = C2SPacket::decode_in_state(State::Play, &mut &buf[..]).expect("decode");
    match decoded {
        C2SPacket::PlayerPosRot(p) => {
            assert_eq!(p.x, 1.0);
            assert_eq!(p.y, 64.5);
            assert_eq!(p.z, -3.25);
            assert_eq!(p.y_rot, 180.0);
            assert_eq!(p.x_rot, 45.0);
            assert!(p.on_ground);
        }
        other => panic!("expected PlayerPosRot, got {other:?}"),
    }
}

#[test]
fn c2s_player_rot_roundtrip() {
    let pkt = C2SPlayerRot {
        y_rot: 90.0,
        x_rot: -10.0,
        on_ground: false,
    };
    let buf = encode(&pkt);

    let decoded = C2SPacket::decode_in_state(State::Play, &mut &buf[..]).expect("decode");
    match decoded {
        C2SPacket::PlayerRot(p) => {
            assert_eq!(p.y_rot, 90.0);
            assert_eq!(p.x_rot, -10.0);
            assert!(!p.on_ground);
        }
        other => panic!("expected PlayerRot, got {other:?}"),
    }
}

#[test]
fn block_pos_roundtrip() {
    // Includes negative coords to exercise sign-extension on unpack.
    for (x, y, z) in [(0, 64, 0), (1, 64, -3), (-30_000_000, -2048, 29_999_999)] {
        let pos = BlockPos::new(x, y, z);
        let mut buf = Vec::new();
        pos.encode(&mut buf).expect("encode");
        assert_eq!(buf.len(), 8); // packed into a single i64
        let decoded = BlockPos::decode(&mut &buf[..]).expect("decode");
        assert_eq!(decoded, pos);
    }
}

// == New core type roundtrip tests == \\

#[test]
fn bitset_empty_roundtrip() {
    let bs = BitSet::empty();
    let mut buf = Vec::new();
    bs.encode(&mut buf).expect("encode");
    // Single VarInt(0) = one byte 0x00.
    assert_eq!(buf, vec![0x00]);
    let decoded = BitSet::decode(&mut &buf[..]).expect("decode");
    assert_eq!(decoded, BitSet::empty());
}

#[test]
fn bitset_with_one_slot_roundtrip() {
    let bs = BitSet::from_slots(vec![1i64 << 5]);
    let mut buf = Vec::new();
    bs.encode(&mut buf).expect("encode");
    // VarInt(1) + 8 bytes = 9 bytes total.
    assert_eq!(buf.len(), 9);
    assert_eq!(buf[0], 0x01); // VarInt(1)
    let decoded = BitSet::decode(&mut &buf[..]).expect("decode");
    assert_eq!(decoded, bs);
}

#[test]
fn f32_f64_roundtrip() {
    let mut buf = Vec::new();
    1.5f32.encode(&mut buf).unwrap();
    let v = f32::decode(&mut &buf[..]).unwrap();
    assert_eq!(v, 1.5);
    buf.clear();
    let neg: f64 = -3.25;
    neg.encode(&mut buf).unwrap();
    let v = f64::decode(&mut &buf[..]).unwrap();
    assert_eq!(v, -3.25);
}

// == Precomputed payload byte-level sanity tests == \\
//
// These verify the byte shape of the limbo's hand-built NBT / chunk
// payloads. They don't roundtrip through Minecraft — they just check that
// the bytes are well-formed NBT / chunk data so the client won't choke on
// them.

use statik_proto::v1_20_1::s2c::play;

#[test]
fn registry_payload_root_nbt_has_u16_name_length() {
    let bytes = play::registry_bytes();

    // Root TAG_Compound + u16 BE 0 for the empty name. (Mojang's
    // FriendlyByteBuf.writeNbt uses DataOutput.writeUTF (u16) for the root
    // tag name; the client reads it with NbtIo.read → input.readUTF → u16.
    // Writing VarInt(0) here shifts the parser by one byte and produces
    // the "Non [a-z0-9_-] character in namespace" error.)
    assert_eq!(bytes[0], 0x0a, "outer tag must be TAG_Compound");
    assert_eq!(
        &bytes[1..3],
        &[0x00, 0x00],
        "root name length must be u16 BE 0 (two bytes), not VarInt(0)"
    );
}

#[test]
fn registry_payload_contains_required_registries() {
    let bytes = play::registry_bytes();
    let text = String::from_utf8_lossy(bytes);

    // The vanilla client requires at least these three registry names to
    // be present in the registry compound.
    for key in [
        "minecraft:dimension_type",
        "minecraft:worldgen/biome",
        "minecraft:chat_type",
    ] {
        assert!(
            text.contains(key),
            "registry must contain {key} but it's missing"
        );
    }
}

#[test]
fn void_chunk_payload_has_correct_layout() {
    let bytes = play::void_chunk_bytes();

    // First 4 bytes: chunk x coordinate (i32 BE) — should be 0.
    assert_eq!(&bytes[..4], &[0, 0, 0, 0]);
    // Next 4 bytes: chunk z coordinate (i32 BE) — should be 0.
    assert_eq!(&bytes[4..8], &[0, 0, 0, 0]);

    // After x, z comes ClientboundLevelChunkPacketData. The first thing in
    // there is the heightmaps NBT, which starts with 0x0A (TAG_Compound).
    assert_eq!(bytes[8], 0x0a, "heightmaps should start with TAG_Compound");
    // The root name length must be u16 BE 0 (two bytes), matching the
    // format the client reads with `DataInput.readUTF`.
    assert_eq!(
        &bytes[9..11],
        &[0x00, 0x00],
        "root name length must be u16 BE 0, not VarInt(0)"
    );
}

#[test]
fn void_chunk_payload_heightmaps_use_i32_lengths() {
    let bytes = play::void_chunk_bytes();

    // Each Long_Array entry has a u16 name followed by an i32 BE length.
    // For 256 longs the i32 BE is "0x00 0x00 0x01 0x00" (4 bytes).
    // Verify this pattern appears at least twice (MOTION_BLOCKING +
    // WORLD_SURFACE).
    let needle = [0x00, 0x00, 0x01, 0x00]; // i32 BE = 256
    let positions: Vec<usize> = bytes
        .windows(needle.len())
        .enumerate()
        .filter(|(_, w)| *w == needle)
        .map(|(i, _)| i)
        .collect();
    assert!(
        positions.len() >= 2,
        "expected at least 2 occurrences of i32 BE 256 (MOTION_BLOCKING + WORLD_SURFACE), found {}",
        positions.len()
    );
}
