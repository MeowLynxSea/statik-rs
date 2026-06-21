use std::borrow::Cow;

#[derive(Debug, Default)]
pub struct RawBytes(pub Cow<'static, [u8]>);

impl RawBytes {
    pub fn new<S: Into<Cow<'static, [u8]>>>(data: S) -> RawBytes {
        RawBytes(data.into())
    }
}
