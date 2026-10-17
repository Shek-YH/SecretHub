//! Model-catalog overlay: strict parsing, validation and merging into the bundled registry.
//! Pure functions only; signing, fetching and caching live elsewhere.

use crate::{models::CAPABILITY_VOCABULARY, CatalogModel, ProviderDefinition};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, sync::LazyLock};

pub const MAX_CATALOG_BYTES: usize = 1024 * 1024;
pub const MAX_MODELS_PER_PROVIDER: usize = 300;
const MAX_ID_CHARS: usize = 200;
const MAX_LABEL_CHARS: usize = 200;
const SUPPORTED_SCHEMA: u32 = 1;

/// Overlay file (`model-catalog.json`). Every struct rejects unknown fields, so a
/// URL or any other extra key makes the whole file invalid.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Overlay {
    pub schema: u32,
    pub serial: u64,
    pub published_at: String,
    pub providers: BTreeMap<String, OverlayProvider>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OverlayProvider {
    #[serde(default)]
    pub default_model: Option<String>,
    pub models: Vec<OverlayModel>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OverlayModel {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub deprecated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogError(pub String);

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "model catalog rejected: {}", self.0)
    }
}

impl std::error::Error for CatalogError {}

fn reject<T>(message: impl Into<String>) -> Result<T, CatalogError> {
    Err(CatalogError(message.into()))
}

fn has_control(value: &str) -> bool {
    value.chars().any(char::is_control)
}

/// Parses and validates an overlay. Providers unknown to the bundled registry are
/// not checked here; `effective_catalog` ignores them.
pub fn parse_overlay(bytes: &[u8]) -> Result<Overlay, CatalogError> {
    if bytes.len() > MAX_CATALOG_BYTES {
        return reject("file exceeds 1 MB");
    }
    let text = std::str::from_utf8(bytes).map_err(|_| CatalogError("not UTF-8".to_owned()))?;
    let overlay: Overlay = serde_json::from_str(text)
        .map_err(|error| CatalogError(format!("invalid JSON: {error}")))?;
    validate(&overlay)?;
    Ok(overlay)
}

fn validate(overlay: &Overlay) -> Result<(), CatalogError> {
    if overlay.schema != SUPPORTED_SCHEMA {
        return reject(format!("unsupported schema {}", overlay.schema));
    }
    if overlay.serial == 0 {
        return reject("serial must be positive");
    }
    let published = overlay.published_at.as_bytes();
    let well_formed = (10..=40).contains(&published.len())
        && published
            .iter()
            .all(|b| b.is_ascii_digit() || b"-:.TZ+".contains(b));
    if !well_formed {
        return reject("publishedAt must be an ISO-8601 timestamp");
    }
    for (provider_id, provider) in &overlay.providers {
        if provider.models.is_empty() {
            return reject(format!("{provider_id}: models must not be empty"));
        }
        if provider.models.len() > MAX_MODELS_PER_PROVIDER {
            return reject(format!(
                "{provider_id}: more than {MAX_MODELS_PER_PROVIDER} models"
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for model in &provider.models {
            let length = model.id.chars().count();
            if length == 0 || length > MAX_ID_CHARS || has_control(&model.id) {
                return reject(format!("{provider_id}: invalid model id"));
            }
            if !seen.insert(model.id.as_str()) {
                return reject(format!("{provider_id}: duplicate model id `{}`", model.id));
            }
            if let Some(label) = &model.label {
                if label.chars().count() > MAX_LABEL_CHARS || has_control(label) {
                    return reject(format!("{provider_id}: invalid label for `{}`", model.id));
                }
            }
            if let Some(unknown) = model
                .capabilities
                .iter()
                .find(|cap| !CAPABILITY_VOCABULARY.contains(&cap.as_str()))
            {
                return reject(format!("{provider_id}: unknown capability `{unknown}`"));
            }
        }
        if let Some(default) = &provider.default_model {
            let usable = provider
                .models
                .iter()
                .any(|model| &model.id == default && !model.deprecated);
            if !usable {
                return reject(format!(
                    "{provider_id}: defaultModel must be a listed, non-deprecated model"
                ));
            }
        }
    }
    Ok(())
}

/// Serial and publish time shipped with this build (`catalog-serial.json`).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BundledSerial {
    pub serial: u64,
    pub published_at: String,
}

static BUNDLED_SERIAL: LazyLock<BundledSerial> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../packages/provider-definitions/catalog-serial.json"
    ))
    .expect("packages/provider-definitions/catalog-serial.json is valid")
});

pub fn bundled_serial() -> &'static BundledSerial {
    &BUNDLED_SERIAL
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CatalogOrigin {
    Bundled,
    Remote,
}

/// Bundled registry with the overlay's model lists applied.
#[derive(Debug, Clone)]
pub struct EffectiveCatalog {
    pub serial: u64,
    pub published_at: String,
    pub origin: CatalogOrigin,
    pub providers: Vec<ProviderDefinition>,
}

/// Per-provider whole replacement of the model list and default. Endpoints, protocols,
/// env names and every other provider field always stay as bundled; unknown providers
/// in the overlay are ignored.
pub fn effective_catalog(
    bundled: &[ProviderDefinition],
    overlay: Option<&Overlay>,
) -> EffectiveCatalog {
    let Some(overlay) = overlay else {
        let serial = bundled_serial();
        return EffectiveCatalog {
            serial: serial.serial,
            published_at: serial.published_at.clone(),
            origin: CatalogOrigin::Bundled,
            providers: bundled.to_vec(),
        };
    };
    let providers = bundled
        .iter()
        .map(|provider| {
            let mut provider = provider.clone();
            if let Some(entry) = overlay.providers.get(&provider.id) {
                provider.models = entry
                    .models
                    .iter()
                    .map(|model| {
                        let mut capabilities: Vec<String> = Vec::new();
                        for cap in &model.capabilities {
                            if !capabilities.contains(cap) {
                                capabilities.push(cap.clone());
                            }
                        }
                        CatalogModel {
                            id: model.id.clone(),
                            label: model.label.clone(),
                            capabilities,
                            deprecated: model.deprecated,
                        }
                    })
                    .collect();
                provider.default_model = entry.default_model.clone();
            }
            provider
        })
        .collect();
    EffectiveCatalog {
        serial: overlay.serial,
        published_at: overlay.published_at.clone(),
        origin: CatalogOrigin::Remote,
        providers,
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveProviderModels {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    pub models: Vec<CatalogModel>,
}

/// Models-only view shared with the frontend and the MCP server (no URLs, no env names).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveModels {
    pub schema: u32,
    pub serial: u64,
    pub published_at: String,
    pub source: CatalogOrigin,
    pub providers: BTreeMap<String, EffectiveProviderModels>,
}

pub fn effective_models(catalog: &EffectiveCatalog) -> EffectiveModels {
    EffectiveModels {
        schema: SUPPORTED_SCHEMA,
        serial: catalog.serial,
        published_at: catalog.published_at.clone(),
        source: catalog.origin,
        providers: catalog
            .providers
            .iter()
            .filter(|provider| !provider.models.is_empty())
            .map(|provider| {
                (
                    provider.id.clone(),
                    EffectiveProviderModels {
                        default_model: provider.default_model.clone(),
                        models: provider.models.clone(),
                    },
                )
            })
            .collect(),
    }
}
