//! Smart .env / JSON import: groups provider variables (key, base URL, model) into one
//! API key credential per provider and leaves everything else as generic entries.
//! Pure functions only; values are carried but never logged or rendered.

use secrethub_exporter::EnvEntry;
use secrethub_providers::ProviderDefinition;
use serde::Serialize;
use std::collections::HashSet;
use url::Url;

/// Largest import file read into memory.
pub const MAX_IMPORT_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedSecret {
    pub name: String,
    pub provider_id: String,
    pub env_key: String,
    pub value: String,
    /// True for a credential synthesized from a known provider (validated after import).
    pub provider_credential: bool,
    pub endpoint_url: String,
    pub endpoint_env_key: String,
    pub model_id: String,
    pub model_env_key: String,
    /// Variable names (never values) this credential was built from, in file order.
    pub source_keys: Vec<String>,
}

/// One preview line; contains variable names only.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreviewRow {
    /// Provider display name; empty for generic entries.
    pub label: String,
    pub generic: bool,
    pub keys: Vec<String>,
}

fn host_of(url: &str) -> Option<String> {
    Url::parse(url.trim())
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_ascii_lowercase))
}

fn first_non_empty<'a>(
    entries: &'a [EnvEntry],
    used: &[bool],
    key: &str,
) -> Option<(usize, &'a str)> {
    entries
        .iter()
        .enumerate()
        .find(|(index, entry)| !used[*index] && entry.key == key && !entry.value.trim().is_empty())
        .map(|(index, entry)| (index, entry.value.trim()))
}

/// Providers that own `key` as their primary API key variable. Only model providers qualify.
fn candidates<'a>(
    registry: &'a [ProviderDefinition],
    key: &str,
) -> Vec<(usize, &'a ProviderDefinition)> {
    registry
        .iter()
        .enumerate()
        .filter(|(_, provider)| {
            provider.model_env_key.is_some()
                && provider.env_keys.first().is_some_and(|first| first == key)
        })
        .collect()
}

/// Shared variable names: imported BASE_URL host first, then featured, then catalog order.
fn pick_provider<'a>(
    options: Vec<(usize, &'a ProviderDefinition)>,
    entries: &[EnvEntry],
    used: &[bool],
) -> &'a ProviderDefinition {
    options
        .into_iter()
        .min_by_key(|(index, provider)| {
            let host_match = provider
                .base_url_env_key
                .as_deref()
                .and_then(|key| first_non_empty(entries, used, key))
                .and_then(|(_, value)| host_of(value))
                .zip(host_of(&provider.base_url))
                .is_some_and(|(imported, catalog)| imported == catalog);
            (!host_match, !provider.featured, *index)
        })
        .map(|(_, provider)| provider)
        .expect("at least one candidate")
}

pub fn plan_import(entries: &[EnvEntry], registry: &[ProviderDefinition]) -> Vec<PlannedSecret> {
    let mut used = vec![false; entries.len()];
    let mut planned = Vec::new();
    for index in 0..entries.len() {
        if used[index] {
            continue;
        }
        let entry = &entries[index];
        let options = candidates(registry, &entry.key);
        if options.is_empty() || entry.value.trim().is_empty() {
            continue;
        }
        let provider = pick_provider(options, entries, &used);
        used[index] = true;
        let mut source = vec![(index, entry.key.clone())];
        let mut claim = |key: Option<&str>, used: &mut Vec<bool>| -> Option<String> {
            let key = key?;
            let (found, value) = first_non_empty(entries, used, key)?;
            used[found] = true;
            source.push((found, key.to_owned()));
            Some(value.to_owned())
        };
        let imported_url = claim(provider.base_url_env_key.as_deref(), &mut used);
        let imported_model = claim(provider.model_env_key.as_deref(), &mut used);
        let preset = crate::capture::build_preset(provider, "");
        source.sort_by_key(|(position, _)| *position);
        planned.push(PlannedSecret {
            name: provider.name.clone(),
            provider_id: provider.id.clone(),
            env_key: entry.key.clone(),
            value: entry.value.clone(),
            provider_credential: true,
            endpoint_url: imported_url.unwrap_or(preset.endpoint_url),
            endpoint_env_key: preset.endpoint_env_key,
            model_id: imported_model.unwrap_or(preset.model_id),
            model_env_key: preset.model_env_key,
            source_keys: source.into_iter().map(|(_, key)| key).collect(),
        });
    }
    // Remaining non-empty entries stay generic (listed after the provider credentials).
    for (index, entry) in entries.iter().enumerate() {
        if used[index] || entry.value.trim().is_empty() {
            continue;
        }
        planned.push(PlannedSecret {
            name: entry.key.clone(),
            provider_id: "generic".to_owned(),
            env_key: entry.key.clone(),
            value: entry.value.clone(),
            provider_credential: false,
            endpoint_url: String::new(),
            endpoint_env_key: String::new(),
            model_id: String::new(),
            model_env_key: String::new(),
            source_keys: vec![entry.key.clone()],
        });
    }
    planned
}

/// Appends " 2", " 3", ... to provider credential names that collide with existing names.
pub fn assign_names(planned: &mut [PlannedSecret], existing: &[String]) {
    let mut taken: HashSet<String> = existing
        .iter()
        .map(|name| name.trim().to_lowercase())
        .collect();
    for secret in planned
        .iter_mut()
        .filter(|secret| secret.provider_credential)
    {
        let base = secret.name.clone();
        let mut candidate = base.clone();
        let mut counter = 2;
        while taken.contains(&candidate.to_lowercase()) {
            candidate = format!("{base} {counter}");
            counter += 1;
        }
        taken.insert(candidate.to_lowercase());
        secret.name = candidate;
    }
}

pub fn preview_rows(
    planned: &[PlannedSecret],
    registry: &[ProviderDefinition],
) -> Vec<ImportPreviewRow> {
    planned
        .iter()
        .map(|secret| ImportPreviewRow {
            label: if secret.provider_credential {
                registry
                    .iter()
                    .find(|provider| provider.id == secret.provider_id)
                    .map_or_else(
                        || secret.provider_id.clone(),
                        |provider| provider.name.clone(),
                    )
            } else {
                String::new()
            },
            generic: !secret.provider_credential,
            keys: secret.source_keys.clone(),
        })
        .collect()
}

/// Reads an import file inside the backend so its contents never reach the WebView.
/// Errors never include file contents.
pub fn read_import_file(path: &str) -> Result<(String, &'static str), String> {
    let path = std::path::Path::new(path);
    let metadata =
        std::fs::metadata(path).map_err(|_| "cannot read the selected file".to_owned())?;
    if !metadata.is_file() {
        return Err("the selected path is not a file".to_owned());
    }
    if metadata.len() > MAX_IMPORT_FILE_BYTES {
        return Err("the file is larger than 1 MB".to_owned());
    }
    let bytes = std::fs::read(path).map_err(|_| "cannot read the selected file".to_owned())?;
    let text =
        String::from_utf8(bytes).map_err(|_| "the file is not valid UTF-8 text".to_owned())?;
    let text = text
        .strip_prefix('\u{feff}')
        .map(str::to_owned)
        .unwrap_or(text);
    let format = if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        "json"
    } else {
        "env"
    };
    Ok((text, format))
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrethub_providers::default_registry;
    use std::io::Write;

    fn entries(pairs: &[(&str, &str)]) -> Vec<EnvEntry> {
        pairs
            .iter()
            .map(|(key, value)| EnvEntry::new(*key, *value))
            .collect()
    }

    fn by_provider<'a>(plan: &'a [PlannedSecret], id: &str) -> &'a PlannedSecret {
        plan.iter()
            .find(|secret| secret.provider_id == id)
            .expect("planned provider")
    }

    #[test]
    fn merges_key_base_url_and_model_into_one_credential() {
        let registry = default_registry();
        let plan = plan_import(
            &entries(&[
                ("OPENAI_API_KEY", "fake-key"),
                ("FOO_TOKEN", "fake-token"),
                ("OPENAI_BASE_URL", "https://proxy.example.com/v1"),
                ("OPENAI_MODEL", "my-model"),
            ]),
            &registry,
        );
        assert_eq!(plan.len(), 2);
        let openai = by_provider(&plan, "openai");
        assert!(openai.provider_credential);
        assert_eq!(openai.name, "OpenAI");
        assert_eq!(openai.env_key, "OPENAI_API_KEY");
        assert_eq!(openai.value, "fake-key");
        assert_eq!(openai.endpoint_url, "https://proxy.example.com/v1");
        assert_eq!(openai.endpoint_env_key, "OPENAI_BASE_URL");
        assert_eq!(openai.model_id, "my-model");
        assert_eq!(openai.model_env_key, "OPENAI_MODEL");
        assert_eq!(
            openai.source_keys,
            ["OPENAI_API_KEY", "OPENAI_BASE_URL", "OPENAI_MODEL"]
        );
        let generic = by_provider(&plan, "generic");
        assert!(!generic.provider_credential);
        assert_eq!(generic.env_key, "FOO_TOKEN");
        let rows = preview_rows(&plan, &registry);
        assert_eq!(rows[0].label, "OpenAI");
        assert_eq!(
            rows[0].keys,
            ["OPENAI_API_KEY", "OPENAI_BASE_URL", "OPENAI_MODEL"]
        );
        assert!(rows[1].generic);
        assert_eq!(rows[1].keys, ["FOO_TOKEN"]);
    }

    #[test]
    fn falls_back_to_catalog_url_and_default_model() {
        let registry = default_registry();
        let plan = plan_import(&entries(&[("ANTHROPIC_API_KEY", "fake")]), &registry);
        let anthropic = by_provider(&plan, "anthropic");
        let definition = registry
            .iter()
            .find(|provider| provider.id == "anthropic")
            .unwrap();
        assert_eq!(anthropic.endpoint_url, definition.base_url);
        assert_eq!(
            anthropic.model_id,
            definition
                .models
                .iter()
                .find(|model| !model.deprecated)
                .unwrap()
                .id
        );
        assert_eq!(anthropic.source_keys, ["ANTHROPIC_API_KEY"]);
    }

    #[test]
    fn shared_variable_names_prefer_base_url_host_then_featured_then_order() {
        let registry = default_registry();
        // BASE_URL host decides between the two MiniMax regions and the two Moonshot regions.
        let plan = plan_import(
            &entries(&[
                ("MINIMAX_API_KEY", "fake"),
                ("MINIMAX_GLOBAL_BASE_URL", "https://api.minimax.io/v1"),
            ]),
            &registry,
        );
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].provider_id, "minimax-global");
        let plan = plan_import(
            &entries(&[
                ("MOONSHOT_API_KEY", "fake"),
                ("MOONSHOT_BASE_URL", "https://api.moonshot.ai/v1"),
            ]),
            &registry,
        );
        assert_eq!(plan[0].provider_id, "kimi-global");
        assert_eq!(plan[0].endpoint_url, "https://api.moonshot.ai/v1");
        // Without a URL the featured provider wins.
        assert_eq!(
            plan_import(&entries(&[("MINIMAX_API_KEY", "fake")]), &registry)[0].provider_id,
            "minimax"
        );
        assert_eq!(
            plan_import(&entries(&[("MOONSHOT_API_KEY", "fake")]), &registry)[0].provider_id,
            "kimi"
        );
        // Neither OpenCode provider is featured: catalog order decides.
        let opencode = plan_import(&entries(&[("OPENCODE_API_KEY", "fake")]), &registry);
        let first = registry
            .iter()
            .find(|provider| {
                provider
                    .env_keys
                    .first()
                    .is_some_and(|key| key == "OPENCODE_API_KEY")
            })
            .unwrap();
        assert_eq!(opencode[0].provider_id, first.id);
    }

    #[test]
    fn leaves_unknown_and_empty_entries_generic_and_numbers_duplicate_names() {
        let registry = default_registry();
        let mut plan = plan_import(
            &entries(&[
                ("OPENAI_API_KEY", "fake-1"),
                ("OPENAI_API_KEY", "fake-2"),
                ("DEEPSEEK_API_KEY", ""),
                ("GITHUB_TOKEN", "fake"),
            ]),
            &registry,
        );
        // Two OpenAI keys and GITHUB_TOKEN as a generic entry; the empty DeepSeek value is skipped.
        assert_eq!(
            plan.iter()
                .filter(|secret| secret.provider_credential)
                .count(),
            2
        );
        assert_eq!(
            plan.iter()
                .filter(|secret| !secret.provider_credential)
                .count(),
            1
        );
        assert!(plan
            .iter()
            .filter(|secret| !secret.provider_credential)
            .all(|secret| secret.provider_id == "generic"));
        assign_names(&mut plan, &["OpenAI".to_owned()]);
        let names: Vec<_> = plan
            .iter()
            .filter(|secret| secret.provider_credential)
            .map(|secret| secret.name.as_str())
            .collect();
        assert_eq!(names, ["OpenAI 2", "OpenAI 3"]);
    }

    #[test]
    fn reads_files_inside_the_backend_with_size_and_encoding_limits() {
        let directory = tempfile::tempdir().unwrap();
        let env_path = directory.path().join(".env");
        std::fs::write(&env_path, "\u{feff}FOO=bar\n").unwrap();
        let (text, format) = read_import_file(env_path.to_str().unwrap()).unwrap();
        assert_eq!((text.as_str(), format), ("FOO=bar\n", "env"));
        let json_path = directory.path().join("keys.JSON");
        std::fs::write(&json_path, "{}").unwrap();
        assert_eq!(
            read_import_file(json_path.to_str().unwrap()).unwrap().1,
            "json"
        );
        let big = directory.path().join("big.env");
        let mut file = std::fs::File::create(&big).unwrap();
        file.write_all(&vec![b'a'; MAX_IMPORT_FILE_BYTES as usize + 1])
            .unwrap();
        assert!(read_import_file(big.to_str().unwrap())
            .unwrap_err()
            .contains("1 MB"));
        let binary = directory.path().join("bin.env");
        std::fs::write(&binary, [0xff, 0xfe, 0x00, 0x81]).unwrap();
        assert!(read_import_file(binary.to_str().unwrap())
            .unwrap_err()
            .contains("UTF-8"));
        assert!(read_import_file(directory.path().to_str().unwrap()).is_err());
    }
}
