use chrono::Utc;
use jsonwebtoken::{
    decode, decode_header, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::secret;
use crate::error::ApiError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenScope {
    Session,
    TenantSelect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub tenant_id: Option<Uuid>,
    pub scope: TokenScope,
    pub iat: i64,
    pub exp: i64,
}

/// One signing secret and the key id that names it in a token header.
#[derive(Debug, Clone)]
pub struct SigningKey {
    pub kid: String,
    pub secret: String,
}

impl SigningKey {
    pub fn new(secret: impl Into<String>) -> Self {
        let secret = secret.into();
        let kid = secret::signing_kid(&secret);

        Self { kid, secret }
    }
}

#[derive(Clone)]
pub struct JwtConfig {
    /// Keys accepted for verification, current first. The rest are previous
    /// secrets kept only for the length of one token TTL, so rotating the
    /// signing secret does not log everyone out at once.
    pub keys: Vec<SigningKey>,
    pub ttl_seconds: i64,
}

impl JwtConfig {
    /// Single-key configuration, which is the steady state.
    pub fn new(secret: impl Into<String>, ttl_seconds: i64) -> Self {
        Self {
            keys: vec![SigningKey::new(secret)],
            ttl_seconds,
        }
    }

    /// Keeps a previous secret acceptable for verification. Signing still uses
    /// the current one.
    pub fn with_previous(mut self, secret: impl Into<String>) -> Self {
        self.keys.push(SigningKey::new(secret));

        self
    }

    pub fn issue(
        &self,
        user_id: Uuid,
        tenant_id: Option<Uuid>,
        scope: TokenScope,
        ttl_seconds: i64,
    ) -> Result<String, ApiError> {
        let now = Utc::now().timestamp();
        let claims = Claims {
            sub: user_id,
            tenant_id,
            scope,
            iat: now,
            exp: now + ttl_seconds,
        };

        let key = self
            .keys
            .first()
            .expect("JwtConfig always carries at least one signing key");

        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some(key.kid.clone());

        encode(
            &header,
            &claims,
            &EncodingKey::from_secret(key.secret.as_bytes()),
        )
        .map_err(ApiError::from)
    }

    pub fn decode(&self, token: &str) -> Result<Claims, ApiError> {
        let header = decode_header(token).map_err(ApiError::from)?;

        // A token names the key that signed it. Tokens issued before key ids
        // existed do not, so they fall back to every key still held.
        let candidates: Vec<&SigningKey> = match &header.kid {
            Some(kid) => self.keys.iter().filter(|key| &key.kid == kid).collect(),
            None => self.keys.iter().collect(),
        };

        if candidates.is_empty() {
            return Err(ApiError::Unauthorized("invalid_token"));
        }

        let mut last_error = None;
        for key in candidates {
            match decode::<Claims>(
                token,
                &DecodingKey::from_secret(key.secret.as_bytes()),
                &Validation::new(Algorithm::HS256),
            ) {
                Ok(data) => return Ok(data.claims),
                Err(error) => last_error = Some(error),
            }
        }

        Err(last_error.map_or(ApiError::Unauthorized("invalid_token"), ApiError::from))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TTL: i64 = 3_600;

    #[test]
    fn a_token_round_trips_through_the_current_key() {
        let jwt = JwtConfig::new("current", TTL);
        let token = jwt
            .issue(
                Uuid::new_v4(),
                Some(Uuid::new_v4()),
                TokenScope::Session,
                TTL,
            )
            .expect("issue");

        assert_eq!(
            jwt.decode(&token).expect("decode").scope,
            TokenScope::Session
        );
    }

    #[test]
    fn a_token_from_the_previous_key_still_verifies_during_the_window() {
        let old = JwtConfig::new("previous", TTL);
        let token = old
            .issue(
                Uuid::new_v4(),
                Some(Uuid::new_v4()),
                TokenScope::Session,
                TTL,
            )
            .expect("issue");

        let rotated = JwtConfig::new("current", TTL).with_previous("previous");

        assert!(
            rotated.decode(&token).is_ok(),
            "a token issued before the rotation must keep working"
        );
    }

    #[test]
    fn signing_after_a_rotation_uses_the_new_key() {
        let rotated = JwtConfig::new("current", TTL).with_previous("previous");
        let token = rotated
            .issue(Uuid::new_v4(), None, TokenScope::TenantSelect, TTL)
            .expect("issue");

        let header = decode_header(&token).expect("header");
        let kid = header.kid.expect("kid");
        let old_only = JwtConfig::new("previous", TTL);

        assert_eq!(kid, secret::signing_kid("current"));
        assert!(
            old_only.decode(&token).is_err(),
            "the previous key must not be able to verify a token signed by the new one"
        );
    }

    #[test]
    fn a_token_from_an_unknown_key_is_rejected() {
        let token = JwtConfig::new("other", TTL)
            .issue(Uuid::new_v4(), None, TokenScope::TenantSelect, TTL)
            .expect("issue");

        let jwt = JwtConfig::new("current", TTL).with_previous("previous");

        assert!(jwt.decode(&token).is_err());
    }

    #[test]
    fn a_token_naming_an_unknown_key_is_rejected_without_trying_the_others() {
        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some("not-a-key".to_string());
        let claims = Claims {
            sub: Uuid::new_v4(),
            tenant_id: None,
            scope: TokenScope::TenantSelect,
            iat: Utc::now().timestamp(),
            exp: Utc::now().timestamp() + TTL,
        };
        let token = encode(&header, &claims, &EncodingKey::from_secret(b"current")).expect("issue");

        let jwt = JwtConfig::new("current", TTL).with_previous("previous");

        assert!(jwt.decode(&token).is_err());
    }

    #[test]
    fn a_token_signed_with_no_key_id_falls_back_to_every_key() {
        // A token minted before key ids were introduced: it carries no kid, so
        // the config has to try each key it holds.
        let claims = Claims {
            sub: Uuid::new_v4(),
            tenant_id: Some(Uuid::new_v4()),
            scope: TokenScope::Session,
            iat: Utc::now().timestamp(),
            exp: Utc::now().timestamp() + TTL,
        };
        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(b"previous"),
        )
        .expect("issue");

        let rotated = JwtConfig::new("current", TTL).with_previous("previous");

        assert_eq!(
            rotated.decode(&token).expect("decode").sub,
            claims.sub,
            "a legacy token must still verify against the previous key"
        );
    }
}
