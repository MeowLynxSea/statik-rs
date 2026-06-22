#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum State {
    Handshake = 0,
    Status = 1,
    Login = 2,
    /// Introduced in 1.20.2. Not selected via the handshake `next_state` field
    /// (that maps only to Status/Login/Transfer — see
    /// [`crate::handshake::ClientIntent`]). Configuration is entered after
    /// `LoginSuccess` once the client sends `Login Acknowledged`.
    Configuration = 3,
    Play = 4,
}
