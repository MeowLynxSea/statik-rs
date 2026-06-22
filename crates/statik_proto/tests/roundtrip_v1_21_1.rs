//! Round-trip tests for the 1.21.1 (protocol 767) proc-macro-derived packet
//! system.
//!
//! Mirrors the 1.20.1 tests: each packet is encoded via its `Packet`-derived
//! `Encode` impl and then decoded via the `PacketGroup`-derived
//! `decode_in_state` for that version. We cover the new Configuration
//! state, the renamed Login `Hello` / Configuration `CookieResponse`, the
//! Configuration C2S `KeepAlive` (i64) and the Play `KeepAlive` (0x18).

use statik_core::prelude::*;
use statik_proto::{
    common::{KnownPack, Tag, TagGroup},
    v1_21_1::{
        c2s::{
            configuration::{
                C2SClientInformation, C2SConfigurationKeepAlive, C2SFinishConfiguration,
                C2SResourcePackResponse, C2SSelectKnownPacks,
            },
            handshake::C2SHandshake,
            login::{C2SHello, C2SLoginAcknowledged},
            play::C2SKeepAlive,
            C2SPacket as C2SPacketV1_21_1,
        },
        s2c::{
            configuration::{
                S2CConfigurationKeepAlive, S2CCustomPayload, S2CFeatureFlags,
                S2CFinishConfiguration, S2CUpdateTags,
            },
            login::S2CLoginSuccess,
            play::{S2CGameEvent, S2CLogin, S2CPlayerAbilities, S2CPlayerPosition},
            S2CPacket as S2CPacketV1_21_1,
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
        protocol_version: VarInt(767),
        server_address: "example.com".to_string(),
        server_port: 25565,
        next_state: ClientIntent::Login,
    };
    let buf = encode(&pkt);

    let decoded =
        C2SPacketV1_21_1::decode_in_state(State::Handshake, &mut &buf[..]).expect("decode");
    match decoded {
        C2SPacketV1_21_1::Handshake(h) => {
            assert_eq!(h.protocol_version.0, 767);
            assert_eq!(h.server_address, "example.com");
            assert_eq!(h.server_port, 25565);
            assert_eq!(h.next_state, ClientIntent::Login);
        }
        other => panic!("expected Handshake, got {other:?}"),
    }
}

#[test]
fn login_hello_roundtrip() {
    // id 0x00 in Login — same id as StatusRequest / Handshake but a
    // different state.
    let pkt = C2SHello {
        name: "Player1".to_string(),
        profile_id: Uuid::nil(),
    };
    let buf = encode(&pkt);

    let decoded = C2SPacketV1_21_1::decode_in_state(State::Login, &mut &buf[..]).expect("decode");
    match decoded {
        C2SPacketV1_21_1::Hello(h) => {
            assert_eq!(h.name, "Player1");
            assert_eq!(h.profile_id, Uuid::nil());
        }
        other => panic!("expected Hello, got {other:?}"),
    }
}

#[test]
fn login_acknowledged_roundtrip() {
    // id 0x03, no fields.
    let pkt = C2SLoginAcknowledged {};
    let buf = encode(&pkt);
    assert_eq!(buf, [0x03]);

    let decoded = C2SPacketV1_21_1::decode_in_state(State::Login, &mut &buf[..]).expect("decode");
    assert!(matches!(decoded, C2SPacketV1_21_1::LoginAcknowledged(_)));
}

#[test]
fn configuration_client_information_roundtrip() {
    let pkt = C2SClientInformation {
        locale: "zh_cn".to_string(),
        view_distance: 8,
        chat_mode: VarInt(0),
        chat_colors: true,
        skin_parts: 127,
        main_hand: VarInt(1),
        text_filtering_enabled: true,
        allows_listing: true,
    };
    let buf = encode(&pkt);

    let decoded =
        C2SPacketV1_21_1::decode_in_state(State::Configuration, &mut &buf[..]).expect("decode");
    match decoded {
        C2SPacketV1_21_1::ClientInformation(c) => {
            assert_eq!(c.locale, "zh_cn");
            assert_eq!(c.view_distance, 8);
            assert_eq!(c.chat_mode, VarInt(0));
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
fn configuration_select_known_packs_roundtrip() {
    let pkt = C2SSelectKnownPacks {
        known_packs: vec![KnownPack {
            namespace: "minecraft".to_string(),
            id: "core".to_string(),
            version: "1.21.1".to_string(),
        }],
    };
    let buf = encode(&pkt);

    let decoded =
        C2SPacketV1_21_1::decode_in_state(State::Configuration, &mut &buf[..]).expect("decode");
    match decoded {
        C2SPacketV1_21_1::SelectKnownPacks(s) => {
            assert_eq!(s.known_packs.len(), 1);
            assert_eq!(s.known_packs[0].namespace, "minecraft");
            assert_eq!(s.known_packs[0].id, "core");
            assert_eq!(s.known_packs[0].version, "1.21.1");
        }
        other => panic!("expected SelectKnownPacks, got {other:?}"),
    }
}

#[test]
fn configuration_finish_configuration_roundtrip() {
    // id 0x03, no fields.
    let pkt = C2SFinishConfiguration {};
    let buf = encode(&pkt);
    assert_eq!(buf, [0x03]);

    let decoded =
        C2SPacketV1_21_1::decode_in_state(State::Configuration, &mut &buf[..]).expect("decode");
    assert!(matches!(
        decoded,
        C2SPacketV1_21_1::FinishConfigurationAck(_)
    ));
}

#[test]
fn configuration_keepalive_roundtrip() {
    // id 0x04 — note the C2S Configuration KeepAlive is `id: i64` (8-byte
    // signed BE), not a VarInt.
    let pkt = C2SConfigurationKeepAlive {
        id: 0x0123_4567_89ab_cdef_i64,
    };
    let buf = encode(&pkt);

    let decoded =
        C2SPacketV1_21_1::decode_in_state(State::Configuration, &mut &buf[..]).expect("decode");
    match decoded {
        C2SPacketV1_21_1::ConfigurationKeepAlive(k) => assert_eq!(k.id, pkt.id),
        other => panic!("expected ConfigurationKeepAlive, got {other:?}"),
    }
}

#[test]
fn play_keepalive_roundtrip() {
    // id 0x18 in Play (1.20.1 had 0x12).
    let pkt = C2SKeepAlive {
        id: 0x0123_4567_89ab_cdef_i64,
    };
    let buf = encode(&pkt);

    let decoded = C2SPacketV1_21_1::decode_in_state(State::Play, &mut &buf[..]).expect("decode");
    match decoded {
        C2SPacketV1_21_1::KeepAlive(k) => assert_eq!(k.id, pkt.id),
        other => panic!("expected KeepAlive, got {other:?}"),
    }
}

#[test]
fn s2c_login_success_roundtrip() {
    // Login 0x02 — 1.21.1 adds a trailing `strict_error_handling: bool`
    // field vs 1.20.1 (verified against PrismarineJS protocol.json).
    let pkt = S2CLoginSuccess {
        uuid: Uuid::nil(),
        username: "Player1".to_string(),
        properties: vec![],
        strict_error_handling: false,
    };
    let buf = encode(&pkt);

    let decoded = S2CPacketV1_21_1::decode_in_state(State::Login, &mut &buf[..]).expect("decode");
    match decoded {
        S2CPacketV1_21_1::LoginSuccess(s) => {
            assert_eq!(s.username, "Player1");
            assert_eq!(s.uuid, Uuid::nil());
            assert!(s.properties.is_empty());
            assert!(!s.strict_error_handling);
        }
        other => panic!("expected LoginSuccess, got {other:?}"),
    }
}

#[test]
fn s2c_login_success_strict_error_handling_is_last_byte() {
    // Regression: the new `strict_error_handling` field must be encoded as
    // the *trailing* byte (PrismarineJS order: uuid, username, properties,
    // strictErrorHandling). If we accidentally insert it earlier or drop
    // it, the byte length / final byte would be wrong.
    let pkt_false = S2CLoginSuccess {
        uuid: Uuid::nil(),
        username: "x".to_string(),
        properties: vec![],
        strict_error_handling: false,
    };
    let pkt_true = S2CLoginSuccess {
        uuid: Uuid::nil(),
        username: "x".to_string(),
        properties: vec![],
        strict_error_handling: true,
    };
    let a = encode(&pkt_false);
    let b = encode(&pkt_true);
    assert_eq!(a.len(), b.len());
    assert_eq!(&a[..a.len() - 1], &b[..b.len() - 1]);
    assert_eq!(a[a.len() - 1], 0x00);
    assert_eq!(b[b.len() - 1], 0x01);
}

#[test]
fn s2c_configuration_feature_flags_roundtrip() {
    let pkt = S2CFeatureFlags {
        features: vec!["minecraft:vanilla".to_string()],
    };
    let buf = encode(&pkt);

    let decoded =
        S2CPacketV1_21_1::decode_in_state(State::Configuration, &mut &buf[..]).expect("decode");
    match decoded {
        S2CPacketV1_21_1::FeatureFlags(f) => {
            assert_eq!(f.features, vec!["minecraft:vanilla".to_string()]);
        }
        other => panic!("expected FeatureFlags, got {other:?}"),
    }
}

#[test]
fn s2c_configuration_finish_configuration_roundtrip() {
    let pkt = S2CFinishConfiguration {};
    let buf = encode(&pkt);
    assert_eq!(buf, [0x03]);

    let decoded =
        S2CPacketV1_21_1::decode_in_state(State::Configuration, &mut &buf[..]).expect("decode");
    assert!(matches!(decoded, S2CPacketV1_21_1::FinishConfiguration(_)));
}

#[test]
fn s2c_configuration_update_tags_empty_roundtrip() {
    // statik ships an empty tag list in production; this is the byte
    // shape the vanilla client receives in limbo.
    let pkt = S2CUpdateTags { tags: vec![] };
    let buf = encode(&pkt);
    // id 0x0D + VarInt(0) tag groups.
    assert_eq!(buf, [0x0d, 0x00]);

    let decoded =
        S2CPacketV1_21_1::decode_in_state(State::Configuration, &mut &buf[..]).expect("decode");
    match decoded {
        S2CPacketV1_21_1::UpdateTags(u) => assert!(u.tags.is_empty()),
        other => panic!("expected UpdateTags, got {other:?}"),
    }
}

#[test]
fn s2c_configuration_update_tags_nonempty_roundtrip() {
    // Exhaustive roundtrip — verifies the nested Vec<TagGroup> /
    // Vec<VarInt> wire shape (two registries × two tags × mixed id
    // counts) decodes back to the same structure.
    let pkt = S2CUpdateTags {
        tags: vec![
            TagGroup {
                tag_type: "minecraft:block".to_string(),
                tags: vec![
                    Tag {
                        name: "minecraft:stone".to_string(),
                        entries: vec![VarInt(1), VarInt(2)],
                    },
                    Tag {
                        name: "minecraft:dirt".to_string(),
                        entries: vec![VarInt(3)],
                    },
                ],
            },
            TagGroup {
                tag_type: "minecraft:item".to_string(),
                tags: vec![Tag {
                    name: "minecraft:pickaxes".to_string(),
                    entries: vec![],
                }],
            },
        ],
    };
    let buf = encode(&pkt);
    assert_eq!(buf[0], 0x0d);

    let decoded =
        S2CPacketV1_21_1::decode_in_state(State::Configuration, &mut &buf[..]).expect("decode");
    let decoded_tags = match decoded {
        S2CPacketV1_21_1::UpdateTags(u) => u.tags,
        other => panic!("expected UpdateTags, got {other:?}"),
    };

    assert_eq!(decoded_tags.len(), 2);
    assert_eq!(decoded_tags[0].tag_type, "minecraft:block");
    assert_eq!(decoded_tags[0].tags.len(), 2);
    assert_eq!(decoded_tags[0].tags[0].name, "minecraft:stone");
    assert_eq!(decoded_tags[0].tags[0].entries, vec![VarInt(1), VarInt(2)]);
    assert_eq!(decoded_tags[0].tags[1].entries, vec![VarInt(3)]);
    assert_eq!(decoded_tags[1].tag_type, "minecraft:item");
    assert_eq!(decoded_tags[1].tags[0].name, "minecraft:pickaxes");
    assert!(decoded_tags[1].tags[0].entries.is_empty());
}

#[test]
fn s2c_configuration_custom_payload_roundtrip() {
    let pkt = S2CCustomPayload {
        channel: "minecraft:brand".to_string(),
        data: RawBytes::new(vec![0x07, b'v', b'a', b'n', b'i', b'l', b'l', b'a']),
    };
    let buf = encode(&pkt);

    let decoded =
        S2CPacketV1_21_1::decode_in_state(State::Configuration, &mut &buf[..]).expect("decode");
    match decoded {
        S2CPacketV1_21_1::CustomPayload(c) => {
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
fn s2c_play_player_position_roundtrip() {
    // id 0x40 in 1.21.1 (1.20.1 had 0x3C). Field layout is identical.
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

    let decoded = S2CPacketV1_21_1::decode_in_state(State::Play, &mut &buf[..]).expect("decode");
    match decoded {
        S2CPacketV1_21_1::PlayerPosition(p) => {
            assert_eq!(p.x, 0.5);
            assert_eq!(p.y, 64.0);
            assert_eq!(p.z, 0.5);
            assert_eq!(p.id, VarInt(0));
        }
        other => panic!("expected PlayerPosition, got {other:?}"),
    }
}

#[test]
fn s2c_play_player_abilities_roundtrip() {
    use statik_proto::common::abilities;
    let pkt = S2CPlayerAbilities {
        flags: abilities::INVULNERABLE | abilities::FLYING | abilities::CAN_FLY,
        flying_speed: 0.05,
        walking_speed: 0.1,
    };
    let buf = encode(&pkt);

    let decoded = S2CPacketV1_21_1::decode_in_state(State::Play, &mut &buf[..]).expect("decode");
    match decoded {
        S2CPacketV1_21_1::PlayerAbilities(a) => {
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
fn s2c_play_game_event_roundtrip() {
    // event = 7 (START_WAITING_FOR_LEVELS) — value to be verified in stage 3.
    let pkt = S2CGameEvent {
        event: 7,
        param: 0.0,
    };
    let buf = encode(&pkt);

    let decoded = S2CPacketV1_21_1::decode_in_state(State::Play, &mut &buf[..]).expect("decode");
    match decoded {
        S2CPacketV1_21_1::GameEvent(g) => {
            assert_eq!(g.event, 7);
            assert_eq!(g.param, 0.0);
        }
        other => panic!("expected GameEvent, got {other:?}"),
    }
}

#[test]
fn s2c_play_login_roundtrip() {
    // 1.21.1 S2CLogin is now fully modeled (no more RawBytes placeholder).
    // Field layout verified against PrismarineJS protocol.json.
    use statik_proto::v1_21_1::s2c::play::SpawnInfo;
    let pkt = S2CLogin {
        entity_id: 0,
        is_hardcore: false,
        world_names: vec!["minecraft:the_void".to_string()],
        max_players: VarInt(20),
        view_distance: VarInt(8),
        simulation_distance: VarInt(8),
        reduced_debug_info: false,
        enable_respawn_screen: false,
        do_limited_crafting: false,
        world_state: SpawnInfo {
            dimension: VarInt(0),
            name: "minecraft:the_void".to_string(),
            hashed_seed: 0,
            gamemode: 1,
            previous_gamemode: 0xff,
            is_debug: false,
            is_flat: true,
            death: None,
            portal_cooldown: VarInt(0),
        },
        enforces_secure_chat: false,
    };
    let buf = encode(&pkt);

    let decoded = S2CPacketV1_21_1::decode_in_state(State::Play, &mut &buf[..]).expect("decode");
    match decoded {
        S2CPacketV1_21_1::Login(p) => {
            assert_eq!(p.entity_id, 0);
            assert!(!p.is_hardcore);
            assert_eq!(p.world_names, vec!["minecraft:the_void".to_string()]);
            assert_eq!(p.max_players.0, 20);
            assert_eq!(p.view_distance.0, 8);
            assert_eq!(p.simulation_distance.0, 8);
            assert_eq!(p.world_state.dimension.0, 0);
            assert_eq!(p.world_state.name, "minecraft:the_void");
            assert_eq!(p.world_state.gamemode, 1);
            assert_eq!(p.world_state.previous_gamemode, 0xff);
            assert!(p.world_state.is_flat);
            assert!(p.world_state.death.is_none());
            assert_eq!(p.world_state.portal_cooldown.0, 0);
            assert!(!p.enforces_secure_chat);
        }
        other => panic!("expected Login, got {other:?}"),
    }
}

#[test]
fn s2c_play_login_field_order_first_byte_after_id() {
    // Regression guard: the very first field after the packet id must be
    // the `entity_id: i32` (4 BE bytes), per PrismarineJS protocol.json.
    // If we reorder, the client mis-interprets these 4 bytes as the
    // hardcore bool + 3 garbage bytes and the connection breaks silently.
    use statik_proto::v1_21_1::s2c::play::SpawnInfo;
    let pkt = S2CLogin {
        entity_id: 0x1122_3344i32,
        is_hardcore: true,
        world_names: vec![],
        max_players: VarInt(0),
        view_distance: VarInt(0),
        simulation_distance: VarInt(0),
        reduced_debug_info: false,
        enable_respawn_screen: false,
        do_limited_crafting: false,
        world_state: SpawnInfo {
            dimension: VarInt(0),
            name: String::new(),
            hashed_seed: 0,
            gamemode: 0,
            previous_gamemode: 0,
            is_debug: false,
            is_flat: false,
            death: None,
            portal_cooldown: VarInt(0),
        },
        enforces_secure_chat: false,
    };
    let buf = encode(&pkt);
    // [id 0x2B] [entity_id BE i32 = 11 22 33 44] [is_hardcore 0x01] ...
    assert_eq!(buf[0], 0x2b);
    assert_eq!(&buf[1..5], &[0x11, 0x22, 0x33, 0x44]);
    assert_eq!(buf[5], 0x01); // is_hardcore = true
}

#[test]
fn configuration_keepalive_id_field_is_i64() {
    // Regression guard: in 1.21.1, the Configuration C2S KeepAlive id is an
    // i64 (8-byte signed BE), NOT a VarInt like its S2C sibling. A VarInt-
    // sized id would silently encode the low 7 bits of a large value and
    // desync the client.
    let pkt = C2SConfigurationKeepAlive {
        id: 0x0123_4567_i64,
    };
    let buf = encode(&pkt);
    // leading id (0x04) + 8 raw bytes = 9 bytes total
    assert_eq!(buf.len(), 9);
    assert_eq!(buf[0], 0x04);
    let id = i64::from_be_bytes(buf[1..9].try_into().unwrap());
    assert_eq!(id, 0x0123_4567);
}

#[test]
fn configuration_keepalive_decode_rejects_varint_length() {
    // A C2S KeepAlive payload that's clearly truncated (fewer than 8 bytes
    // after the leading 0x04 id) must be rejected. The previous versions
    // would silently accept this and read garbage.
    let bad = [0x04u8, 0x00, 0x01, 0x02]; // 3 bytes after id, not 8
    let result = C2SPacketV1_21_1::decode_in_state(State::Configuration, &mut &bad[..]);
    assert!(
        result.is_err(),
        "expected decode to fail on truncated i64 body"
    );
}

#[test]
fn s2c_configuration_keepalive_is_i64() {
    // Regression: PrismarineJS protocol.json (configuration.toClient.
    // packet_keep_alive) shows `keepAliveId: i64`. We had this as VarInt
    // before — that would underflow against the real client. Verify the
    // wire is exactly 1 byte id + 8 bytes BE i64.
    let pkt = S2CConfigurationKeepAlive {
        keep_alive_id: 0x0123_4567_89ab_cdefi64,
    };
    let buf = encode(&pkt);
    // 1 byte id + 8 bytes payload = 9 bytes total
    assert_eq!(buf.len(), 9, "wire size must be 1+8 bytes");
    assert_eq!(buf[0], 0x04);
    let id = i64::from_be_bytes(buf[1..9].try_into().unwrap());
    assert_eq!(id, 0x0123_4567_89ab_cdefi64);

    let decoded =
        S2CPacketV1_21_1::decode_in_state(State::Configuration, &mut &buf[..]).expect("decode");
    match decoded {
        S2CPacketV1_21_1::ConfigurationKeepAlive(k) => {
            assert_eq!(k.keep_alive_id, 0x0123_4567_89ab_cdefi64)
        }
        other => panic!("expected ConfigurationKeepAlive, got {other:?}"),
    }
}

#[test]
fn c2s_resource_pack_response_uuid_first() {
    // Regression: PrismarineJS protocol.json (configuration.toServer.
    // packet_resource_pack_receive) lists `uuid: UUID + result: varint`.
    // Earlier we only had `result: varint`, which would have mis-decoded
    // the per-request UUID as a (huge) VarInt and yielded garbage.
    let pkt = C2SResourcePackResponse {
        uuid: Uuid::from_u128(0xdead_beef_dead_beef_dead_beef_dead_beefu128),
        result: VarInt(3),
    };
    let buf = encode(&pkt);
    // 1 byte id + 16 byte UUID + 1 byte VarInt(3) = 18 bytes
    assert_eq!(buf.len(), 1 + 16 + 1);
    assert_eq!(buf[0], 0x06);

    let decoded =
        C2SPacketV1_21_1::decode_in_state(State::Configuration, &mut &buf[..]).expect("decode");
    match decoded {
        C2SPacketV1_21_1::ResourcePackResponse(r) => {
            assert_eq!(r.uuid, pkt.uuid);
            assert_eq!(r.result.0, 3);
        }
        other => panic!("expected ResourcePackResponse, got {other:?}"),
    }
}

#[test]
fn registry_blobs_parse_as_varint_count_plus_entries() {
    // Sanity: each precomputed registry blob starts with a VarInt(count)
    // and contains exactly `count` triples of (String key, bool present,
    // anonymous-NBT compound) — verified by walking the first byte of
    // each entry's NBT root to confirm it's TAG_Compound (0x0A).
    use std::io::Read;

    use statik_core::prelude::*;
    for (registry_id, blob_fn) in statik_proto::v1_21_1::registries::all() {
        let blob = blob_fn();
        let mut cur = blob;
        let count = VarInt::decode(&mut cur).expect("varint count").0;
        assert!(count >= 0, "registry {registry_id}: negative count {count}");
        // We don't fully walk the NBT (that'd duplicate the builder), but
        // we do check that every entry starts with a String key whose
        // length is sane, followed by a 0x01 (present) bool, followed by
        // a 0x0A (TAG_Compound) NBT root.
        // We also bail if we accidentally encoded a length-zero key (a
        // sign of mis-encoding).
        for i in 0..count {
            let key = String::decode(&mut cur)
                .unwrap_or_else(|e| panic!("registry {registry_id} entry {i}: key decode: {e}"));
            assert!(
                !key.is_empty(),
                "registry {registry_id} entry {i}: empty key"
            );
            let mut present = [0u8; 1];
            cur.read_exact(&mut present).expect("present bool");
            assert_eq!(
                present[0], 0x01,
                "registry {registry_id} entry {i}: present byte should be 0x01"
            );
            let mut tag = [0u8; 1];
            cur.read_exact(&mut tag).expect("nbt tag byte");
            assert_eq!(
                tag[0], 0x0a,
                "registry {registry_id} entry {i}: NBT root tag should be 0x0A (Compound), got \
                 0x{:02X}",
                tag[0]
            );
            // Skip over the rest of this compound by counting nested
            // TAG_Compound depth, paying attention to the END byte.
            let mut depth: i32 = 1;
            while depth > 0 {
                let mut t = [0u8; 1];
                cur.read_exact(&mut t).expect("nbt walk");
                let tag = t[0];
                if tag == 0x00 {
                    depth -= 1;
                    continue;
                }
                // Read tag name (u16 BE length + UTF-8 bytes).
                let mut nlen_buf = [0u8; 2];
                cur.read_exact(&mut nlen_buf).expect("nbt name len");
                let nlen = u16::from_be_bytes(nlen_buf) as usize;
                let mut name = vec![0u8; nlen];
                cur.read_exact(&mut name).expect("nbt name");
                // Skip the payload.
                skip_nbt_payload(&mut cur, tag, &mut depth);
            }
        }
        assert!(
            cur.is_empty(),
            "registry {registry_id}: {} trailing bytes after {count} entries",
            cur.len()
        );
    }

    // Walk a single NBT payload (no name; tag already consumed).
    fn skip_nbt_payload(cur: &mut &[u8], tag: u8, depth: &mut i32) {
        use std::io::Read;
        match tag {
            0x01 => {
                let mut b = [0u8; 1];
                cur.read_exact(&mut b).unwrap();
            }
            0x02 => {
                let mut b = [0u8; 2];
                cur.read_exact(&mut b).unwrap();
            }
            0x03 | 0x05 => {
                let mut b = [0u8; 4];
                cur.read_exact(&mut b).unwrap();
            }
            0x04 | 0x06 => {
                let mut b = [0u8; 8];
                cur.read_exact(&mut b).unwrap();
            }
            0x07 => {
                let mut len = [0u8; 4];
                cur.read_exact(&mut len).unwrap();
                let n = i32::from_be_bytes(len) as usize;
                let mut sink = vec![0u8; n];
                cur.read_exact(&mut sink).unwrap();
            }
            0x08 => {
                let mut len = [0u8; 2];
                cur.read_exact(&mut len).unwrap();
                let n = u16::from_be_bytes(len) as usize;
                let mut sink = vec![0u8; n];
                cur.read_exact(&mut sink).unwrap();
            }
            0x09 => {
                let mut it = [0u8; 1];
                cur.read_exact(&mut it).unwrap();
                let inner_tag = it[0];
                let mut len = [0u8; 4];
                cur.read_exact(&mut len).unwrap();
                let n = i32::from_be_bytes(len);
                if inner_tag == 0x00 {
                    return;
                }
                for _ in 0..n {
                    if inner_tag == 0x0a {
                        *depth += 1;
                        // Walk this nested compound by re-entering the
                        // outer loop's logic — read named tags until END.
                        loop {
                            let mut t = [0u8; 1];
                            cur.read_exact(&mut t).unwrap();
                            let nt = t[0];
                            if nt == 0x00 {
                                *depth -= 1;
                                break;
                            }
                            let mut nlen_buf = [0u8; 2];
                            cur.read_exact(&mut nlen_buf).unwrap();
                            let nlen = u16::from_be_bytes(nlen_buf) as usize;
                            let mut name = vec![0u8; nlen];
                            cur.read_exact(&mut name).unwrap();
                            skip_nbt_payload(cur, nt, depth);
                        }
                    } else {
                        skip_nbt_payload(cur, inner_tag, depth);
                    }
                }
            }
            0x0a => {
                // Nested compound — re-enter the named-tag walker.
                loop {
                    let mut t = [0u8; 1];
                    cur.read_exact(&mut t).unwrap();
                    let nt = t[0];
                    if nt == 0x00 {
                        break;
                    }
                    let mut nlen_buf = [0u8; 2];
                    cur.read_exact(&mut nlen_buf).unwrap();
                    let nlen = u16::from_be_bytes(nlen_buf) as usize;
                    let mut name = vec![0u8; nlen];
                    cur.read_exact(&mut name).unwrap();
                    skip_nbt_payload(cur, nt, depth);
                }
            }
            0x0b => {
                let mut len = [0u8; 4];
                cur.read_exact(&mut len).unwrap();
                let n = i32::from_be_bytes(len) as usize;
                let mut sink = vec![0u8; n * 4];
                cur.read_exact(&mut sink).unwrap();
            }
            0x0c => {
                let mut len = [0u8; 4];
                cur.read_exact(&mut len).unwrap();
                let n = i32::from_be_bytes(len) as usize;
                let mut sink = vec![0u8; n * 8];
                cur.read_exact(&mut sink).unwrap();
            }
            other => panic!("unknown NBT tag 0x{other:02X}"),
        }
    }
}

// == Precomputed void-chunk payload == \\

/// The 1.21.1 heightmaps NBT is **anonymous** (no u16 length=0 root name
/// prefix). The 1.20.1 payload writes a u16 length=0 root name right after
/// the `0x0A` TAG_Compound tag byte; emitting that to a 1.21.1 client
/// causes it to mis-parse the heightmaps and the entire packet downstream,
/// which the client surfaces as
/// `Failed to decode packet 'clientbound/minecraft:level_chunk_with_light'`.
#[test]
fn void_chunk_payload_uses_anonymous_heightmaps_nbt() {
    use statik_proto::v1_21_1::s2c::play;
    let bytes = play::void_chunk_bytes_v1_21_1();

    // First 4 bytes: chunk x coordinate (i32 BE) — should be 0.
    assert_eq!(&bytes[..4], &[0, 0, 0, 0]);
    // Next 4 bytes: chunk z coordinate (i32 BE) — should be 0.
    assert_eq!(&bytes[4..8], &[0, 0, 0, 0]);

    // After x, z the heightmaps field begins. For 1.21.1 this is an
    // anonymous TAG_Compound: tag byte `0x0A` followed directly by field
    // entries — NO u16 length=0 root-name prefix (which is the 1.20.1
    // shape).
    assert_eq!(bytes[8], 0x0a, "heightmaps should start with TAG_Compound");
    // The next byte after the tag byte must be a field-tag byte, not a
    // zero root-name length. The first field is MOTION_BLOCKING, a
    // TAG_Long_Array (0x0C).
    assert_eq!(
        bytes[9], 0x0c,
        "expected MOTION_BLOCKING TAG_Long_Array (0x0C) immediately after the heightmaps tag byte \
         — the 1.21.1 heightmaps is anonymous, so a u16 length=0 root-name prefix would be a \
         1.20.1 leak",
    );
}

/// Smoke test that the 1.21.1 payload decodes past the heightmaps and
/// lands on the expected chunk-data length of 192 bytes (24 overworld
/// sections × 8 bytes each = 2-byte block_count + 3-byte block-states
/// paletted container + 3-byte biomes paletted container).
///
/// Walks the anonymous heightmaps compound by hand instead of hardcoding
/// byte offsets, so it's robust to future changes in the heightmaps
/// payload (e.g. renaming the two long arrays).
#[test]
fn void_chunk_payload_v1_21_1_top_level_shape() {
    use std::io::{Cursor, Read};

    use statik_proto::v1_21_1::s2c::play;
    let bytes = play::void_chunk_bytes_v1_21_1();

    let mut cur = Cursor::new(bytes);

    // Skip x (4) and z (4).
    let mut four = [0u8; 4];
    cur.read_exact(&mut four).unwrap();
    cur.read_exact(&mut four).unwrap();
    assert_eq!(four, [0, 0, 0, 0]);
    assert_eq!(
        &cur.get_ref()[cur.position() as usize - 4..cur.position() as usize],
        &[0, 0, 0, 0]
    );

    // Anonymous heightmaps TAG_Compound: read field entries until TAG_End.
    let mut tag = [0u8; 1];
    cur.read_exact(&mut tag).unwrap();
    assert_eq!(tag[0], 0x0a, "heightmaps should start with TAG_Compound");
    loop {
        cur.read_exact(&mut tag).unwrap();
        if tag[0] == 0x00 {
            break;
        }
        // u16 name length + name bytes.
        let mut nlen = [0u8; 2];
        cur.read_exact(&mut nlen).unwrap();
        let n = u16::from_be_bytes(nlen) as usize;
        let mut name = vec![0u8; n];
        cur.read_exact(&mut name).unwrap();
        // Skip the field payload based on its tag.
        match tag[0] {
            0x0c => {
                // TAG_Long_Array: i32 BE length, then `length * 8` bytes.
                let mut lbuf = [0u8; 4];
                cur.read_exact(&mut lbuf).unwrap();
                let l = i32::from_be_bytes(lbuf) as usize;
                let mut sink = vec![0u8; l * 8];
                cur.read_exact(&mut sink).unwrap();
            }
            other => panic!("unexpected NBT tag 0x{other:02X} in heightmaps"),
        }
    }

    // Chunk data length: VarInt(192) = [0xC0, 0x01] (2 bytes — 192 >= 128).
    let mut v1 = [0u8; 1];
    cur.read_exact(&mut v1).unwrap();
    assert_eq!(v1[0] & 0x80, 0x80, "192 should encode as a 2-byte VarInt");
    let mut v2 = [0u8; 1];
    cur.read_exact(&mut v2).unwrap();
    let chunk_data_len = ((v1[0] & 0x7f) as i32) | ((v2[0] as i32) << 7);
    assert_eq!(
        chunk_data_len, 192,
        "chunk data buffer should be 192 bytes (24 sections × 8 bytes)"
    );

    // Skip the chunk data and the block-entities count (VarInt 0).
    let mut sink = vec![0u8; 192];
    cur.read_exact(&mut sink).unwrap();
    let mut be = [0u8; 1];
    cur.read_exact(&mut be).unwrap();
    assert_eq!(be[0], 0x00, "block entities count should be 0");

    // Four empty light masks + two empty light-update arrays.
    for label in [
        "skyLightMask",
        "blockLightMask",
        "emptySkyLightMask",
        "emptyBlockLightMask",
        "skyLight",
        "blockLight",
    ] {
        cur.read_exact(&mut be).unwrap();
        assert_eq!(be[0], 0x00, "{label} should be an empty i64[]varint");
    }

    // And that should be the entire payload.
    assert_eq!(
        cur.position() as usize,
        bytes.len(),
        "no trailing bytes expected"
    );
}
