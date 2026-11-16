//! Bundled invocation templates (`invocations` / `docsUrl` in providers.json).
//! They ship with the app only: the remote catalog overlay rejects them (see `catalog.rs`).

use crate::{models::CAPABILITY_VOCABULARY, ProviderDefinition};
use serde::{Deserialize, Serialize};
use url::Url;

pub const INVOCATION_KINDS: [&str; 9] = [
    "openai-chat",
    "openai-images",
    "openai-audio-speech",
    "openai-audio-transcriptions",
    "openai-embeddings",
    "anthropic-messages",
    "gemini-generate",
    "async-task",
    "native",
];
const MAX_PATH_CHARS: usize = 200;

/// How to call one capability. `path` is relative to the provider's bundled `baseUrl`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Invocation {
    pub kind: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_path: Option<String>,
}

fn check_path(label: &str, path: &str) -> Result<(), String> {
    let ok = path.starts_with('/')
        && path.chars().count() <= MAX_PATH_CHARS
        && !path.contains("//")
        && !path.contains("..")
        && !path.contains(':')
        && !path.chars().any(char::is_whitespace);
    if ok {
        Ok(())
    } else {
        Err(format!("{label} `{path}` is not a safe relative path"))
    }
}

/// Checks one provider's `invocations` and `docsUrl`; returns the first problem.
pub fn validate_invocations(provider: &ProviderDefinition) -> Result<(), String> {
    let id = &provider.id;
    for (capability, invocation) in &provider.invocations {
        if !CAPABILITY_VOCABULARY.contains(&capability.as_str()) {
            return Err(format!("{id}: unknown capability `{capability}`"));
        }
        if !INVOCATION_KINDS.contains(&invocation.kind.as_str()) {
            return Err(format!(
                "{id}.{capability}: unknown kind `{}`",
                invocation.kind
            ));
        }
        check_path(&format!("{id}.{capability}: path"), &invocation.path)?;
        if let Some(poll) = &invocation.poll_path {
            if invocation.kind != "async-task" {
                return Err(format!(
                    "{id}.{capability}: pollPath is only for async-task"
                ));
            }
            // `{id}` is the one allowed placeholder; its braces are checked after substitution.
            check_path(
                &format!("{id}.{capability}: pollPath"),
                &poll.replace("{id}", "ID"),
            )?;
        }
    }
    if let Some(docs) = &provider.docs_url {
        let https = Url::parse(docs)
            .map(|url| url.scheme() == "https" && url.host_str().is_some())
            .unwrap_or(false);
        if !https {
            return Err(format!("{id}: docsUrl must be an https URL"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::default_registry;

    fn provider(json: &str) -> ProviderDefinition {
        serde_json::from_str(&format!(
            r#"{{"id":"p","name":"P","envKeys":["P_API_KEY"],"baseUrl":"https://api.example.com/v1",{json}}}"#
        ))
        .expect("fixture parses")
    }

    #[test]
    fn bundled_registry_invocations_are_valid() {
        let registry = default_registry();
        let mut entries = 0;
        for provider in &registry {
            validate_invocations(provider).unwrap_or_else(|error| panic!("{error}"));
            entries += provider.invocations.len();
        }
        assert!(entries > 0);
        let openai = registry.iter().find(|p| p.id == "openai").unwrap();
        assert_eq!(openai.invocations["tts"].path, "/audio/speech");
    }

    #[test]
    fn rejects_bad_invocations() {
        let ok = provider(
            r#""invocations":{"chat":{"kind":"openai-chat","path":"/chat/completions"}},"docsUrl":"https://docs.example.com""#,
        );
        assert_eq!(validate_invocations(&ok), Ok(()));
        let async_ok = provider(
            r#""invocations":{"video_gen":{"kind":"async-task","path":"/tasks","pollPath":"/tasks/{id}"}}"#,
        );
        assert_eq!(validate_invocations(&async_ok), Ok(()));
        let bad = [
            r#""invocations":{"telepathy":{"kind":"openai-chat","path":"/x"}}"#,
            r#""invocations":{"chat":{"kind":"soap","path":"/x"}}"#,
            r#""invocations":{"chat":{"kind":"openai-chat","path":"x"}}"#,
            r#""invocations":{"chat":{"kind":"openai-chat","path":"//evil.example/x"}}"#,
            r#""invocations":{"chat":{"kind":"openai-chat","path":"/a/../b"}}"#,
            r#""invocations":{"chat":{"kind":"openai-chat","path":"/a:b"}}"#,
            r#""invocations":{"chat":{"kind":"openai-chat","path":"/a b"}}"#,
            r#""invocations":{"chat":{"kind":"openai-chat","path":"/a?x=1","pollPath":"/p"}}"#,
            r#""invocations":{"video_gen":{"kind":"async-task","path":"/t","pollPath":"/t/{id}/../x"}}"#,
            r#""docsUrl":"http://docs.example.com""#,
            r#""docsUrl":"javascript:alert(1)""#,
        ];
        let too_long = format!(
            r#""invocations":{{"chat":{{"kind":"openai-chat","path":"/{}"}}}}"#,
            "a".repeat(200)
        );
        for json in bad.iter().copied().chain([too_long.as_str()]) {
            assert!(validate_invocations(&provider(json)).is_err(), "{json}");
        }
    }
}
