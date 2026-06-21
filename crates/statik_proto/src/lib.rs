pub mod c2s;
pub mod s2c;

pub mod prelude {

    pub use crate::{
        c2s::{handshake::*, login::*, play::*, status::*, C2SPacket},
        s2c::{login::*, play::*, status::*, S2CPacket},
    };
}
