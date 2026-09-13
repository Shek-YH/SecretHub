//! Short-lived token and endpoint policy primitives for the optional local Agent Proxy.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::{rngs::OsRng, RngCore};
use secrethub_providers::{default_registry, validate_endpoint};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenGrant {
    pub token: String,
    pub expires_at: u64,
}

pub struct TokenAuthority {
    grants: HashMap<[u8; 32], u64>,
}

impl TokenAuthority {
    pub fn new() -> Self {
        Self {
            grants: HashMap::new(),
        }
    }

    pub fn issue(&mut self, now: u64, ttl_seconds: u64) -> TokenGrant {
        let mut raw = [0u8; 32];
        OsRng.fill_bytes(&mut raw);
        let token = URL_SAFE_NO_PAD.encode(raw);
        let expires_at = now.saturating_add(ttl_seconds);
        self.grants.insert(hash_token(&token), expires_at);
        TokenGrant { token, expires_at }
    }

    pub fn authorize(&mut self, token: &str, now: u64) -> bool {
        self.grants.retain(|_, expires_at| *expires_at > now);
        self.grants
            .get(&hash_token(token))
            .is_some_and(|expires_at| *expires_at > now)
    }

    pub fn revoke(&mut self, token: &str) -> bool {
        self.grants.remove(&hash_token(token)).is_some()
    }
    pub fn active_grants(&self) -> usize {
        self.grants.len()
    }
}

impl Default for TokenAuthority {
    fn default() -> Self {
        Self::new()
    }
}

pub struct EndpointPolicy {
    provider_id: String,
}

impl EndpointPolicy {
    pub fn new(provider_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
        }
    }
    pub fn authorize(&self, endpoint: &str) -> Result<(), String> {
        validate_endpoint(&self.provider_id, endpoint, &default_registry()).map(|_| ())
    }
}

fn hash_token(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}
