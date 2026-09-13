//! Profiles and project scoping primitives.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Profile { pub id: String, pub name: String, pub secret_ids: Vec<String> }

impl Profile {
    pub fn new(name: impl Into<String>, secret_ids: Vec<String>) -> Self { Self { id: format!("profile-{}", uuid::Uuid::new_v4()), name: name.into(), secret_ids } }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ApplyResult { pub available_ids: Vec<String>, pub missing_ids: Vec<String> }

pub fn apply_profile(profile: &Profile, available_ids: &[String]) -> ApplyResult {
    let available = profile.secret_ids.iter().filter(|id| available_ids.contains(id)).cloned().collect();
    let missing = profile.secret_ids.iter().filter(|id| !available_ids.contains(id)).cloned().collect();
    ApplyResult { available_ids: available, missing_ids: missing }
}
