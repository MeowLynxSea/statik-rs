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
    QueryBlockNbt(C2SQueryBlockNbt),
    ChangeDifficulty(C2SChangeDifficulty),
    ChatAck(C2SChatAck),
    ChatCommand(C2SChatCommand),
    ChatMessage(C2SChatMessage),
    ChatSessionUpdate(C2SChatSessionUpdate),
    ClientCommand(C2SClientCommand),
    ClientInformation(C2SClientInformation),
    CommandSuggestion(C2SCommandSuggestion),
    EnchantItem(C2SEnchantItem),
    ContainerClick(C2SContainerClick),
    ContainerClose(C2SContainerClose),
    CustomPayload(C2SCustomPayload),
    EditBook(C2SEditBook),
    QueryEntityNbt(C2SQueryEntityNbt),
    Interact(C2SInteract),
    GenerateStructure(C2SGenerateStructure),
    KeepAlive(C2SKeepAlive),
    LockDifficulty(C2SLockDifficulty),
    PlayerPos(C2SPlayerPos),
    PlayerPosRot(C2SPlayerPosRot),
    PlayerRot(C2SPlayerRot),
    PlayerStatusOnly(C2SPlayerStatusOnly),
    MoveVehicle(C2SMoveVehicle),
    PaddleBoat(C2SPaddleBoat),
    PickItem(C2SPickItem),
    PlaceRecipe(C2SPlaceRecipe),
    PlayerAbilities(C2SPlayerAbilities),
    PlayerAction(C2SPlayerAction),
    PlayerCommand(C2SPlayerCommand),
    PlayerInput(C2SPlayerInput),
    Pong(C2SPong),
    RecipeBookChangeSettings(C2SRecipeBookChangeSettings),
    RecipeBookSeenRecipe(C2SRecipeBookSeenRecipe),
    RenameItem(C2SRenameItem),
    ResourcePack(C2SResourcePack),
    SeenAdvancements(C2SSeenAdvancements),
    SelectTrade(C2SSelectTrade),
    SetBeacon(C2SSetBeacon),
    SetCarriedItem(C2SSetCarriedItem),
    SetCommandBlock(C2SSetCommandBlock),
    SetCommandMinecart(C2SSetCommandMinecart),
    SetCreativeModeSlot(C2SSetCreativeModeSlot),
    SetJigsawBlock(C2SSetJigsawBlock),
    SetStructureBlock(C2SSetStructureBlock),
    SignUpdate(C2SSignUpdate),
    Swing(C2SSwing),
    Spectate(C2SSpectate),
    UseItemOn(C2SUseItemOn),
    UseItem(C2SUseItem),
}
