use secrethub_core::{NewSecretInput, VaultService};
use secrethub_storage::Database;
use serde::Deserialize;
use std::{env, path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("SecretHub CLI error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".to_owned());
    let database = Database::open(database_path()).map_err(|error| error.to_string())?;
    let mut vault = VaultService::open(database).map_err(|error| error.to_string())?;
    match command.as_str() {
        "status" => {
            println!(
                "initialized={} unlocked={}",
                vault.is_initialized().map_err(|error| error.to_string())?,
                vault.is_unlocked()
            );
            Ok(())
        }
        "catalog" => {
            unlock_existing(&mut vault)?;
            for secret in vault.list_metadata().map_err(|error| error.to_string())? {
                println!(
                    "{}\t{}\t{}\t{}",
                    secret.id, secret.name, secret.env_key, secret.status
                );
            }
            Ok(())
        }
        "create" => {
            unlock_or_initialize(&mut vault)?;
            let name = required(&mut args, "name")?;
            let env_key = required(&mut args, "env-key")?;
            let provider = args.next().unwrap_or_else(|| "generic".to_owned());
            let value = rpassword::prompt_password("Secret value (input hidden): ")
                .map_err(|error| error.to_string())?;
            vault
                .create_secret(NewSecretInput {
                    name,
                    provider_id: provider,
                    env_key,
                    description: String::new(),
                    tags: Vec::new(),
                    value,
                })
                .map_err(|error| error.to_string())?;
            println!("secret-created");
            Ok(())
        }
        "run" => {
            let mut spec = parse_run_args(args.collect())?;
            if spec.secret_ids.is_empty() {
                let config = load_project_config()?;
                if !config.inherit_global
                    && config
                        .project
                        .as_deref()
                        .unwrap_or_default()
                        .trim()
                        .is_empty()
                {
                    return Err(
                        ".secrethub.toml with inherit_global=false must declare project".to_owned(),
                    );
                }
                spec.secret_ids = config.keys;
            }
            if spec.secret_ids.is_empty() {
                return Err(
                    "run requires at least one --secret-id or a .secrethub.toml keys list"
                        .to_owned(),
                );
            }
            unlock_existing(&mut vault)?;
            let metadata = vault.list_metadata().map_err(|error| error.to_string())?;
            let mut child = std::process::Command::new(&spec.program);
            child.args(&spec.args);
            for id in spec.secret_ids {
                let item = metadata
                    .iter()
                    .find(|item| item.id == id || item.env_key == id)
                    .ok_or_else(|| format!("secret not found: {id}"))?;
                let value = vault
                    .read_secret(&item.id)
                    .map_err(|error| error.to_string())?;
                child.env(&item.env_key, value);
            }
            let status = child.status().map_err(|error| error.to_string())?;
            if status.success() {
                Ok(())
            } else {
                Err(format!("child process exited with {status}"))
            }
        }
        "help" | "--help" | "-h" => {
            println!("secrethub-cli status\nsecrethub-cli catalog\nsecrethub-cli create <name> <env-key> [provider]\nsecrethub-cli run --secret-id <id> [--secret-id <id>] -- <program> [args...]\n\nSecret values are read without echo and never printed.");
            Ok(())
        }
        _ => Err(format!("unknown command: {command}; use --help")),
    }
}

fn required(args: &mut impl Iterator<Item = String>, name: &str) -> Result<String, String> {
    args.next().ok_or_else(|| format!("missing {name}"))
}

fn unlock_existing(vault: &mut VaultService) -> Result<(), String> {
    let password =
        rpassword::prompt_password("Master Password: ").map_err(|error| error.to_string())?;
    vault.unlock(&password).map_err(|error| error.to_string())
}

fn unlock_or_initialize(vault: &mut VaultService) -> Result<(), String> {
    if vault.is_initialized().map_err(|error| error.to_string())? {
        unlock_existing(vault)
    } else {
        let password = rpassword::prompt_password("Create Master Password: ")
            .map_err(|error| error.to_string())?;
        vault
            .setup_master(&password)
            .map_err(|error| error.to_string())
    }
}

fn database_path() -> PathBuf {
    env::var_os("SECRETHUB_DB_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("com.secrethub.desktop")
                .join("vault.sqlite")
        })
}

struct RunSpec {
    secret_ids: Vec<String>,
    program: String,
    args: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ProjectConfig {
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    keys: Vec<String>,
    #[serde(default)]
    inherit_global: bool,
}

fn load_project_config() -> Result<ProjectConfig, String> {
    let path = PathBuf::from(".secrethub.toml");
    let contents = std::fs::read_to_string(&path)
        .map_err(|_| "no --secret-id supplied and .secrethub.toml was not found".to_owned())?;
    toml::from_str(&contents).map_err(|error| format!("invalid .secrethub.toml: {error}"))
}

fn parse_run_args(args: Vec<String>) -> Result<RunSpec, String> {
    let separator = args
        .iter()
        .position(|arg| arg == "--")
        .ok_or_else(|| "run requires `--` before the child command".to_owned())?;
    let mut secret_ids = Vec::new();
    let mut index = 0;
    while index < separator {
        if args[index] != "--secret-id" {
            return Err(format!("unknown run option: {}", args[index]));
        }
        let id = args
            .get(index + 1)
            .ok_or_else(|| "--secret-id requires an ID".to_owned())?;
        secret_ids.push(id.clone());
        index += 2;
    }
    let program = args
        .get(separator + 1)
        .cloned()
        .ok_or_else(|| "run requires a child command".to_owned())?;
    Ok(RunSpec {
        secret_ids,
        program,
        args: args[separator + 2..].to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::parse_run_args;
    use crate::ProjectConfig;

    #[test]
    fn parses_secret_ids_without_invoking_a_shell() {
        let spec = parse_run_args(vec![
            "--secret-id".into(),
            "secret-1".into(),
            "--".into(),
            "node".into(),
            "script.js".into(),
        ])
        .unwrap();
        assert_eq!(spec.secret_ids, vec!["secret-1"]);
        assert_eq!(spec.program, "node");
        assert_eq!(spec.args, vec!["script.js"]);
    }

    #[test]
    fn accepts_a_project_config_without_any_secret_value_field() {
        let config: ProjectConfig = toml::from_str(
            "project = \"demo\"\nkeys = [\"OPENAI_API_KEY\"]\ninherit_global = false",
        )
        .unwrap();
        assert_eq!(config.project.as_deref(), Some("demo"));
        assert_eq!(config.keys, vec!["OPENAI_API_KEY"]);
        assert!(!config.inherit_global);
    }
}
