//! Client-to-server packets for the Play state (protocol 763, MC 1.20.1).
//!
//! Every C2S Play packet is modeled here so the connection's decode loop can
//! cleanly parse whatever a vanilla client sends, rather than erroring on an
//! unknown id. statik only *acts* on a handful (teleport ack, keep-alive,
//! movement); the rest are decoded and ignored, but modeling them keeps the
//! wire framing honest and the logs readable.
//!
//! Field types follow the authoritative `minecraft-data` protocol schema
//! (`tmp/protocol.json`), NOT the readme's Java `Raw Type` column — see
//! CLAUDE.md. A few packets carry types we don't have first-class encoders
//! for (the `ItemStack`/`slot` type, and the conditional `switch` fields in
//! Interact / Advancement Tab); for those the trailing, statik-irrelevant
//! payload is captured as [`RawBytes`].

use statik_core::prelude::*;
use statik_derive::*;
use uuid::Uuid;

/// 0x00 - Accept Teleportation (Teleport Confirm).
///
/// Acks a `Synchronize Player Position`. `id` must match the teleport id we
/// sent; we just note the ack.
#[derive(Debug, Packet)]
#[packet(id = 0x00, state = State::Play)]
pub struct C2SAcceptTeleportation {
    pub id: VarInt,
}

/// 0x01 - Query Block Entity Tag.
#[derive(Debug, Packet)]
#[packet(id = 0x01, state = State::Play)]
pub struct C2SQueryBlockNbt {
    pub transaction_id: VarInt,
    pub location: BlockPos,
}

/// 0x02 - Change Difficulty.
#[derive(Debug, Packet)]
#[packet(id = 0x02, state = State::Play)]
pub struct C2SChangeDifficulty {
    pub new_difficulty: u8,
}

/// 0x03 - Chat Acknowledgement.
#[derive(Debug, Packet)]
#[packet(id = 0x03, state = State::Play)]
pub struct C2SChatAck {
    pub count: VarInt,
}

/// A single argument signature in [`C2SChatCommand`]: the argument name plus a
/// fixed 256-byte signature.
#[derive(Debug, Encode, Decode)]
pub struct ArgumentSignature {
    pub argument_name: String,
    pub signature: [u8; 256],
}

/// 0x04 - Chat Command.
#[derive(Debug, Packet)]
#[packet(id = 0x04, state = State::Play)]
pub struct C2SChatCommand {
    pub command: String,
    pub timestamp: i64,
    pub salt: i64,
    pub argument_signatures: Vec<ArgumentSignature>,
    pub message_count: VarInt,
    /// 3-byte "acknowledged" bitset of the last seen messages.
    pub acknowledged: [u8; 3],
}

/// 0x05 - Chat Message.
#[derive(Debug, Packet)]
#[packet(id = 0x05, state = State::Play)]
pub struct C2SChatMessage {
    pub message: String,
    pub timestamp: i64,
    pub salt: i64,
    pub signature: Option<[u8; 256]>,
    pub offset: VarInt,
    pub acknowledged: [u8; 3],
}

/// 0x06 - Chat Session Update.
#[derive(Debug, Packet)]
#[packet(id = 0x06, state = State::Play)]
pub struct C2SChatSessionUpdate {
    pub session_uuid: Uuid,
    pub expire_time: i64,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

/// 0x07 - Client Command (e.g. respawn / request stats).
#[derive(Debug, Packet)]
#[packet(id = 0x07, state = State::Play)]
pub struct C2SClientCommand {
    pub action_id: VarInt,
}

/// 0x08 - Client Information (client settings).
///
/// Sent right after entering Play (and whenever options change).
/// `view_distance` is a single signed byte; `chat_visibility` / `main_hand` are
/// enums sent as `VarInt`; `skin_parts` is a `u8` bitmask.
#[derive(Debug, Packet)]
#[packet(id = 0x08, state = State::Play)]
pub struct C2SClientInformation {
    pub language: String,
    pub view_distance: i8,
    pub chat_visibility: VarInt,
    pub chat_colors: bool,
    pub skin_parts: u8,
    pub main_hand: VarInt,
    pub text_filtering_enabled: bool,
    pub allows_listing: bool,
}

/// 0x09 - Command Suggestion (tab-complete request).
#[derive(Debug, Packet)]
#[packet(id = 0x09, state = State::Play)]
pub struct C2SCommandSuggestion {
    pub transaction_id: VarInt,
    pub text: String,
}

/// 0x0A - Enchant Item (click an enchantment in the table).
#[derive(Debug, Packet)]
#[packet(id = 0x0A, state = State::Play)]
pub struct C2SEnchantItem {
    pub window_id: i8,
    pub enchantment: i8,
}

/// 0x0B - Container Click (window click).
///
/// Trailing `changedSlots` array and `cursorItem` use the `ItemStack` type,
/// which statik doesn't model; captured as [`RawBytes`].
#[derive(Debug, Packet)]
#[packet(id = 0x0B, state = State::Play)]
pub struct C2SContainerClick {
    pub window_id: u8,
    pub state_id: VarInt,
    pub slot: i16,
    pub mouse_button: i8,
    pub mode: VarInt,
    pub rest: RawBytes,
}

/// 0x0C - Container Close.
#[derive(Debug, Packet)]
#[packet(id = 0x0C, state = State::Play)]
pub struct C2SContainerClose {
    pub window_id: u8,
}

/// 0x0D - Custom Payload (plugin message, e.g. `minecraft:brand`).
///
/// `data` is the remaining frame bytes ([`RawBytes`]).
#[derive(Debug, Packet)]
#[packet(id = 0x0D, state = State::Play)]
pub struct C2SCustomPayload {
    pub channel: String,
    pub data: RawBytes,
}

/// 0x0E - Edit Book.
#[derive(Debug, Packet)]
#[packet(id = 0x0E, state = State::Play)]
pub struct C2SEditBook {
    pub hand: VarInt,
    pub pages: Vec<String>,
    pub title: Option<String>,
}

/// 0x0F - Query Entity Tag.
#[derive(Debug, Packet)]
#[packet(id = 0x0F, state = State::Play)]
pub struct C2SQueryEntityNbt {
    pub transaction_id: VarInt,
    pub entity_id: VarInt,
}

/// 0x10 - Interact (Use Entity).
///
/// The position / hand fields are conditional on `mouse` (the interaction
/// type), which the derive can't express; the variable tail is captured as
/// [`RawBytes`]. statik ignores this packet anyway.
#[derive(Debug, Packet)]
#[packet(id = 0x10, state = State::Play)]
pub struct C2SInteract {
    pub target: VarInt,
    pub mouse: VarInt,
    pub rest: RawBytes,
}

/// 0x11 - Generate Structure (jigsaw).
#[derive(Debug, Packet)]
#[packet(id = 0x11, state = State::Play)]
pub struct C2SGenerateStructure {
    pub location: BlockPos,
    pub levels: VarInt,
    pub keep_jigsaws: bool,
}

/// 0x12 - Keep Alive. The client echoes the id we sent in `S2CKeepAlive`.
#[derive(Debug, Packet)]
#[packet(id = 0x12, state = State::Play)]
pub struct C2SKeepAlive {
    pub id: i64,
}

/// 0x13 - Lock Difficulty.
#[derive(Debug, Packet)]
#[packet(id = 0x13, state = State::Play)]
pub struct C2SLockDifficulty {
    pub locked: bool,
}

/// 0x14 - Move Player Pos.
///
/// Only the position changed. The three "Move Player" packets share Java base
/// fields but each serializes only a subset on the wire (no `hasPos`/`hasRot`
/// booleans — the id determines which fields are present). statik ignores
/// movement; flying mode keeps the client from falling.
#[derive(Debug, Packet)]
#[packet(id = 0x14, state = State::Play)]
pub struct C2SPlayerPos {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub on_ground: bool,
}

/// 0x15 - Move Player Pos Rot. Position + rotation.
#[derive(Debug, Packet)]
#[packet(id = 0x15, state = State::Play)]
pub struct C2SPlayerPosRot {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub y_rot: f32,
    pub x_rot: f32,
    pub on_ground: bool,
}

/// 0x16 - Move Player Rot. Rotation only.
#[derive(Debug, Packet)]
#[packet(id = 0x16, state = State::Play)]
pub struct C2SPlayerRot {
    pub y_rot: f32,
    pub x_rot: f32,
    pub on_ground: bool,
}

/// 0x17 - Move Player Status Only (just the `onGround` flag).
#[derive(Debug, Packet)]
#[packet(id = 0x17, state = State::Play)]
pub struct C2SPlayerStatusOnly {
    pub on_ground: bool,
}

/// 0x18 - Move Vehicle.
#[derive(Debug, Packet)]
#[packet(id = 0x18, state = State::Play)]
pub struct C2SMoveVehicle {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub y_rot: f32,
    pub x_rot: f32,
}

/// 0x19 - Paddle Boat.
#[derive(Debug, Packet)]
#[packet(id = 0x19, state = State::Play)]
pub struct C2SPaddleBoat {
    pub left_paddle: bool,
    pub right_paddle: bool,
}

/// 0x1A - Pick Item.
#[derive(Debug, Packet)]
#[packet(id = 0x1A, state = State::Play)]
pub struct C2SPickItem {
    pub slot: VarInt,
}

/// 0x1B - Place Recipe (craft recipe request).
#[derive(Debug, Packet)]
#[packet(id = 0x1B, state = State::Play)]
pub struct C2SPlaceRecipe {
    pub window_id: i8,
    pub recipe: String,
    pub make_all: bool,
}

/// 0x1C - Player Abilities (client toggles flying). Single byte bitfield.
#[derive(Debug, Packet)]
#[packet(id = 0x1C, state = State::Play)]
pub struct C2SPlayerAbilities {
    pub flags: i8,
}

/// 0x1D - Player Action (block dig: start/stop/finish digging, drop, etc.).
#[derive(Debug, Packet)]
#[packet(id = 0x1D, state = State::Play)]
pub struct C2SPlayerAction {
    pub status: VarInt,
    pub location: BlockPos,
    pub face: i8,
    pub sequence: VarInt,
}

/// 0x1E - Player Command (sprint / sneak / start jump with horse, etc.).
#[derive(Debug, Packet)]
#[packet(id = 0x1E, state = State::Play)]
pub struct C2SPlayerCommand {
    pub entity_id: VarInt,
    pub action_id: VarInt,
    pub jump_boost: VarInt,
}

/// 0x1F - Player Input (vehicle steering: sideways/forward + jump/unmount).
#[derive(Debug, Packet)]
#[packet(id = 0x1F, state = State::Play)]
pub struct C2SPlayerInput {
    pub sideways: f32,
    pub forward: f32,
    pub flags: u8,
}

/// 0x20 - Pong (reply to a Play-state Ping). `id` is a plain BE i32.
#[derive(Debug, Packet)]
#[packet(id = 0x20, state = State::Play)]
pub struct C2SPong {
    pub id: i32,
}

/// 0x21 - Recipe Book Change Settings.
#[derive(Debug, Packet)]
#[packet(id = 0x21, state = State::Play)]
pub struct C2SRecipeBookChangeSettings {
    pub book_id: VarInt,
    pub book_open: bool,
    pub filter_active: bool,
}

/// 0x22 - Recipe Book Seen Recipe (displayed recipe).
#[derive(Debug, Packet)]
#[packet(id = 0x22, state = State::Play)]
pub struct C2SRecipeBookSeenRecipe {
    pub recipe_id: String,
}

/// 0x23 - Rename Item (in an anvil).
#[derive(Debug, Packet)]
#[packet(id = 0x23, state = State::Play)]
pub struct C2SRenameItem {
    pub name: String,
}

/// 0x24 - Resource Pack response.
#[derive(Debug, Packet)]
#[packet(id = 0x24, state = State::Play)]
pub struct C2SResourcePack {
    pub result: VarInt,
}

/// 0x25 - Seen Advancements (advancement tab).
///
/// The `tabId` field is present only when `action == 0` (opened tab); that
/// conditional tail is captured as [`RawBytes`].
#[derive(Debug, Packet)]
#[packet(id = 0x25, state = State::Play)]
pub struct C2SSeenAdvancements {
    pub action: VarInt,
    pub rest: RawBytes,
}

/// 0x26 - Select Trade (villager merchant).
#[derive(Debug, Packet)]
#[packet(id = 0x26, state = State::Play)]
pub struct C2SSelectTrade {
    pub slot: VarInt,
}

/// 0x27 - Set Beacon Effect.
#[derive(Debug, Packet)]
#[packet(id = 0x27, state = State::Play)]
pub struct C2SSetBeacon {
    pub primary_effect: Option<VarInt>,
    pub secondary_effect: Option<VarInt>,
}

/// 0x28 - Set Carried Item (hotbar slot select).
#[derive(Debug, Packet)]
#[packet(id = 0x28, state = State::Play)]
pub struct C2SSetCarriedItem {
    pub slot: i16,
}

/// 0x29 - Set Command Block.
#[derive(Debug, Packet)]
#[packet(id = 0x29, state = State::Play)]
pub struct C2SSetCommandBlock {
    pub location: BlockPos,
    pub command: String,
    pub mode: VarInt,
    pub flags: u8,
}

/// 0x2A - Set Command Minecart.
#[derive(Debug, Packet)]
#[packet(id = 0x2A, state = State::Play)]
pub struct C2SSetCommandMinecart {
    pub entity_id: VarInt,
    pub command: String,
    pub track_output: bool,
}

/// 0x2B - Set Creative Mode Slot.
///
/// `item` is an `ItemStack`; captured as [`RawBytes`].
#[derive(Debug, Packet)]
#[packet(id = 0x2B, state = State::Play)]
pub struct C2SSetCreativeModeSlot {
    pub slot: i16,
    pub item: RawBytes,
}

/// 0x2C - Set Jigsaw Block.
#[derive(Debug, Packet)]
#[packet(id = 0x2C, state = State::Play)]
pub struct C2SSetJigsawBlock {
    pub location: BlockPos,
    pub name: String,
    pub target: String,
    pub pool: String,
    pub final_state: String,
    pub joint_type: String,
}

/// 0x2D - Set Structure Block.
#[derive(Debug, Packet)]
#[packet(id = 0x2D, state = State::Play)]
pub struct C2SSetStructureBlock {
    pub location: BlockPos,
    pub action: VarInt,
    pub mode: VarInt,
    pub name: String,
    pub offset_x: i8,
    pub offset_y: i8,
    pub offset_z: i8,
    pub size_x: i8,
    pub size_y: i8,
    pub size_z: i8,
    pub mirror: VarInt,
    pub rotation: VarInt,
    pub metadata: String,
    pub integrity: f32,
    pub seed: VarInt,
    pub flags: u8,
}

/// 0x2E - Sign Update.
#[derive(Debug, Packet)]
#[packet(id = 0x2E, state = State::Play)]
pub struct C2SSignUpdate {
    pub location: BlockPos,
    pub is_front_text: bool,
    pub line1: String,
    pub line2: String,
    pub line3: String,
    pub line4: String,
}

/// 0x2F - Swing (arm animation).
#[derive(Debug, Packet)]
#[packet(id = 0x2F, state = State::Play)]
pub struct C2SSwing {
    pub hand: VarInt,
}

/// 0x30 - Teleport To Entity (spectate).
#[derive(Debug, Packet)]
#[packet(id = 0x30, state = State::Play)]
pub struct C2SSpectate {
    pub target: Uuid,
}

/// 0x31 - Use Item On (block place).
#[derive(Debug, Packet)]
#[packet(id = 0x31, state = State::Play)]
pub struct C2SUseItemOn {
    pub hand: VarInt,
    pub location: BlockPos,
    pub direction: VarInt,
    pub cursor_x: f32,
    pub cursor_y: f32,
    pub cursor_z: f32,
    pub inside_block: bool,
    pub sequence: VarInt,
}

/// 0x32 - Use Item.
#[derive(Debug, Packet)]
#[packet(id = 0x32, state = State::Play)]
pub struct C2SUseItem {
    pub hand: VarInt,
    pub sequence: VarInt,
}
