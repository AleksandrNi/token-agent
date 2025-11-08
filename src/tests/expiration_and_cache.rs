#[cfg(test)]
mod test {

    use crate::cache::{token::Token, token_cache::TokenCache, token_context::TokenContext};
    use chrono::Utc;
    use std::time::Duration;

    #[tokio::test]
    async fn token_expiration_and_safety_margin_behavior() {
        
        let now = Utc::now().timestamp() as u64;
        let ttl = 4;
        // token that expires in 3 seconds
        let token_context = TokenContext::new(
            "id".to_string(),
            Token::new("short-val".into(), now + ttl),
            now + 1,
        );

        let _ = TokenCache::set("src_short".to_string(), vec![token_context.clone()]).await;

        let got = TokenCache::get("src_short", "id").await;
        assert!(got.is_some());
        assert_eq!(got.unwrap().token.value, "short-val");


        tokio::time::sleep(Duration::from_secs(ttl)).await;
        let got2 = TokenCache::get("src_short", token_context.id.as_str()).await;

        assert!(got2.is_none() == true);
  
    }
}
