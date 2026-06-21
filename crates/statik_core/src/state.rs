#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum State {
    Handshake = 0,
    Status = 1,
    Login = 2,
    Play = 3,
}
