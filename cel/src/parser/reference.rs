#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    String(String),
    Boolean(bool),
    Int(i64),
    UInt(u64),
    Double(f64),
    Bytes(Vec<u8>),
    Null,
}
