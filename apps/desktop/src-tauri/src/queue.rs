//! Shared plumbing for the MCP queue files (`sync-`, `capture-`, `media-`): id checks,
//! pending-file listing and atomic result files. Plans (`plan-`) and capability suggestions
//! (`caps-`) keep their own readers; the prefixes keep every reader away from the others' files.

use crate::plan::{prune_results, RESULT_RETENTION};
use serde::Serialize;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// `<prefix><safe chars>`, at most 128 bytes; keeps ids from escaping the queue directories.
pub fn valid_id(id: &str, prefix: &str) -> bool {
    id.starts_with(prefix)
        && id.len() > prefix.len()
        && id.len() <= 128
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

/// `(id, path)` of every `<prefix>*.json` file in `directory`, unordered.
pub fn pending_files(directory: &Path, prefix: &str) -> Vec<(String, PathBuf)> {
    let Ok(read) = fs::read_dir(directory) else {
        return Vec::new();
    };
    read.filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                return None;
            }
            let stem = path.file_stem()?.to_str()?.to_owned();
            valid_id(&stem, prefix).then_some((stem, path))
        })
        .collect()
}

/// Writes `<directory>/<request_id>.json` atomically, then keeps only the newest results.
pub fn write_result<T: Serialize>(
    directory: &Path,
    request_id: &str,
    prefix: &str,
    value: &T,
) -> io::Result<()> {
    if !valid_id(request_id, prefix) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid request id",
        ));
    }
    fs::create_dir_all(directory)?;
    let json = serde_json::to_string(value).map_err(io::Error::other)?;
    let target = directory.join(format!("{request_id}.json"));
    let temporary = directory.join(format!("{request_id}.json.tmp"));
    fs::write(&temporary, format!("{json}\n"))?;
    fs::rename(&temporary, &target)?;
    prune_results(directory, RESULT_RETENTION);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_need_their_own_prefix_and_a_safe_charset() {
        assert!(valid_id("sync-1760000000000-ab12", "sync-"));
        for bad in ["", "sync-", "sync-../x", "sync-a/b", "plan-1", "caps-1"] {
            assert!(!valid_id(bad, "sync-"), "{bad}");
        }
        assert!(!valid_id(&format!("sync-{}", "a".repeat(130)), "sync-"));
        // Other readers never accept a queue id.
        assert!(!crate::plan::valid_request_id("sync-1"));
        assert!(!crate::caps::valid_request_id("capture-1"));
        assert!(!valid_id("media-1", "capture-"));
    }

    #[test]
    fn lists_only_matching_json_files_and_writes_results_atomically() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "sync-1.json",
            "sync-2.json.tmp",
            "plan-1.json",
            "caps-1.json",
            "sync-x.txt",
        ] {
            fs::write(dir.path().join(name), "{}").unwrap();
        }
        let listed = pending_files(dir.path(), "sync-");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].0, "sync-1");
        write_result(
            dir.path(),
            "sync-1",
            "sync-",
            &serde_json::json!({"status": "done"}),
        )
        .unwrap();
        assert!(write_result(dir.path(), "plan-1", "sync-", &serde_json::json!({})).is_err());
        let text = fs::read_to_string(dir.path().join("sync-1.json")).unwrap();
        assert!(text.contains("\"done\""));
        assert!(!dir.path().join("sync-1.json.tmp").exists());
    }
}
