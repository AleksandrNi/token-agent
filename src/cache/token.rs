
pub const TOKEN_VALUE_STUB: &'static str  = "";

#[derive(Debug, Clone)]
pub struct Token {
    pub value: String,
    pub exp_unix_ts: u64, // UNIX TIMESTAMP

}

impl Token {
    pub fn new(value: String, exp_unix_ts: u64) -> Self {
        Self {value, exp_unix_ts}
    }
}