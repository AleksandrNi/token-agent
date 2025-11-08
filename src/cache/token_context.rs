use chrono::Utc;
use crate::cache::token::Token;

/// Token structure
#[derive(Debug, Clone)]
pub struct TokenContext {
    pub id: String,                     // unique token id per source
    pub token: Token,                   // token
    /// token fetching start at
    pub fetched_at_unix_ts: u64,        // unix seconds
}

impl TokenContext {
    pub fn new(
        id: String, 
        token: Token, 
        safety_margin_seconds: u64, 
    ) -> Self {
        let mut fetched_at_unix_ts = token.exp_unix_ts as i64 - safety_margin_seconds as i64;
        if fetched_at_unix_ts < 0 {
            fetched_at_unix_ts = 0;
        }
        Self {
            id,
            token,
            fetched_at_unix_ts: fetched_at_unix_ts as u64,
        }
    }
    
    /// Check if token should be udtated
    pub fn should_update(&self) -> bool {
        Utc::now().timestamp() as u64 >=  self.fetched_at_unix_ts
    }
    /// Check if token should be removed
    pub fn should_remove_at(&self) -> i64 {
        // invalidate token 1 second before expiration
        (self.token.exp_unix_ts - 1) as i64
    }

    
    /// Check if token is expired
    pub fn should_remove(&self) -> bool {
        Utc::now().timestamp() as u64 >= self.should_remove_at() as u64
    }
}