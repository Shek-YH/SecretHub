//! Media library (design M4): files are copied into `<app data>/media`, deduplicated by sha256,
//! described by metadata and tags, browsable while the vault is locked. Media is not secret, so
//! nothing here needs the vault key. AI additions arrive as `media-*.json` queue files and are
//! copied in only after the user confirms the filename, size and full path.

use crate::{queue, AppState};
use base64::Engine;
use secrethub_core::VaultService;
use secrethub_storage::MediaAsset;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager, State};

const PREFIX: &str = "media-";
const KIND: &str = "add_media";
pub const MAX_MEDIA_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_THUMBNAIL_BYTES: u64 = 10 * 1024 * 1024;
const MAX_IMPORT_PATHS: usize = 200;
const MAX_DESCRIPTION_CHARS: usize = 2000;
const MAX_TAGS: usize = 20;
const FFPROBE_TIMEOUT: Duration = Duration::from_secs(15);
const FFPROBE_CACHE: Duration = Duration::from_secs(30);

#[derive(Debug, PartialEq, Eq)]
pub struct MediaError {
    pub code: &'static str,
    pub message: String,
}

impl MediaError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for MediaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl From<secrethub_core::VaultError> for MediaError {
    fn from(error: secrethub_core::VaultError) -> Self {
        Self::new("storage", error.to_string())
    }
}

fn io_error(error: std::io::Error) -> MediaError {
    MediaError::new("io", error.to_string())
}

// ------------------------------------------------------------ file typing

fn extension_of(filename: &str) -> String {
    Path::new(filename)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 10
                && value.chars().all(|ch| ch.is_ascii_alphanumeric())
        })
        .unwrap_or_default()
}

/// Kind from the file extension: image | video | audio | text | other.
pub fn kind_for(filename: &str) -> &'static str {
    match extension_of(filename).as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "avif" | "heic"
        | "heif" | "ico" | "svg" => "image",
        "mp4" | "mov" | "mkv" | "webm" | "avi" | "m4v" | "wmv" | "flv" | "mpg" | "mpeg" => "video",
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "opus" | "wma" | "aiff" => "audio",
        "txt" | "md" | "markdown" | "json" | "csv" | "srt" | "vtt" | "log" | "yaml" | "yml"
        | "xml" | "html" => "text",
        _ => "other",
    }
}

pub fn mime_for(filename: &str) -> &'static str {
    match extension_of(filename).as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "avif" => "image/avif",
        "heic" => "image/heic",
        "heif" => "image/heif",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "wmv" => "video/x-ms-wmv",
        "flv" => "video/x-flv",
        "mpg" | "mpeg" => "video/mpeg",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "aac" => "audio/aac",
        "ogg" | "opus" => "audio/ogg",
        "m4a" => "audio/mp4",
        "wma" => "audio/x-ms-wma",
        "aiff" => "audio/aiff",
        "txt" | "log" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "json" => "application/json",
        "csv" => "text/csv",
        "srt" | "vtt" => "text/plain",
        "yaml" | "yml" => "application/yaml",
        "xml" => "application/xml",
        "html" => "text/html",
        _ => "application/octet-stream",
    }
}

/// File name for display: no path parts or control characters, at most 120 characters.
pub fn sanitize_filename(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = last.chars().filter(|ch| !ch.is_control()).collect();
    let cleaned = cleaned.trim().chars().take(120).collect::<String>();
    if cleaned.is_empty() {
        "untitled".to_owned()
    } else {
        cleaned
    }
}

fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    for tag in tags {
        let tag = tag.trim();
        let length = tag.chars().count();
        if length == 0
            || length > 32
            || tag.contains(',')
            || tag.chars().any(char::is_control)
            || result
                .iter()
                .any(|existing| existing.to_lowercase() == tag.to_lowercase())
        {
            continue;
        }
        result.push(tag.to_owned());
        if result.len() == MAX_TAGS {
            break;
        }
    }
    result
}

fn clean_description(description: &str) -> String {
    description
        .trim()
        .chars()
        .take(MAX_DESCRIPTION_CHARS)
        .collect()
}

// ---------------------------------------------------------------- probing

#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub struct Probe {
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
}

fn probe_image(path: &Path) -> Probe {
    imagesize::size(path)
        .map(|size| Probe {
            width: Some(size.width as i64),
            height: Some(size.height as i64),
            duration_ms: None,
        })
        .unwrap_or_default()
}

/// Reads `ffprobe -print_format json -show_entries format=duration:stream=codec_type,width,height`.
pub fn parse_ffprobe(output: &str) -> Probe {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(output) else {
        return Probe::default();
    };
    let number = |value: &serde_json::Value| {
        value
            .as_f64()
            .or_else(|| value.as_str().and_then(|text| text.parse::<f64>().ok()))
    };
    let video = json["streams"]
        .as_array()
        .and_then(|streams| {
            streams
                .iter()
                .find(|stream| stream["codec_type"] == "video" && stream["width"].is_number())
        })
        .cloned();
    let duration = number(&json["format"]["duration"])
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
        .map(|seconds| (seconds * 1000.0).round() as i64);
    Probe {
        width: video.as_ref().and_then(|stream| stream["width"].as_i64()),
        height: video.as_ref().and_then(|stream| stream["height"].as_i64()),
        duration_ms: duration,
    }
}

fn ffprobe_command() -> Command {
    let mut command = Command::new("ffprobe");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command
}

/// Whether `ffprobe` is on PATH; cached for a short time so listing stays cheap.
pub fn ffprobe_available() -> bool {
    static CACHE: Mutex<Option<(Instant, bool)>> = Mutex::new(None);
    let mut cache = CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((at, value)) = *cache {
        if at.elapsed() < FFPROBE_CACHE {
            return value;
        }
    }
    let value = ffprobe_command()
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    *cache = Some((Instant::now(), value));
    value
}

fn probe_with_ffprobe(path: &Path) -> Probe {
    let Ok(mut child) = ffprobe_command()
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_entries",
            "format=duration:stream=codec_type,width,height",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Probe::default();
    };
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < FFPROBE_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Probe::default();
            }
        }
    }
    let mut output = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut output);
    }
    parse_ffprobe(&output)
}

/// Dimensions for images; duration and resolution for audio and video when ffprobe exists.
pub fn probe(path: &Path, kind: &str) -> Probe {
    match kind {
        "image" => probe_image(path),
        "video" | "audio" if ffprobe_available() => probe_with_ffprobe(path),
        _ => Probe::default(),
    }
}

// ----------------------------------------------------------------- staging

/// A file copied into the media directory under a temporary name, hashed but not yet recorded.
#[derive(Debug)]
pub struct Staged {
    pub id: String,
    pub tmp: PathBuf,
    pub filename: String,
    pub sha256: String,
    pub size: u64,
}

impl Staged {
    pub fn discard(self) {
        let _ = fs::remove_file(self.tmp);
    }
}

/// The source must be an existing regular file of 1 byte to `max_bytes`; returns its size.
pub fn validate_source(path: &Path, max_bytes: u64) -> Result<u64, MediaError> {
    let metadata = fs::metadata(path)
        .map_err(|_| MediaError::new("not_found", "the file does not exist or cannot be read"))?;
    if !metadata.is_file() {
        return Err(MediaError::new(
            "not_a_file",
            "the path is not a regular file",
        ));
    }
    if metadata.len() == 0 {
        return Err(MediaError::new("empty_file", "the file is empty"));
    }
    if metadata.len() > max_bytes {
        return Err(MediaError::new(
            "too_large",
            format!("the file is larger than {} bytes", max_bytes),
        ));
    }
    Ok(metadata.len())
}

fn new_asset_id() -> String {
    format!("asset-{}", uuid::Uuid::new_v4())
}

/// Copies `source` into `media_dir` while hashing it in the same pass.
pub fn stage_copy(media_dir: &Path, source: &Path, max_bytes: u64) -> Result<Staged, MediaError> {
    validate_source(source, max_bytes)?;
    fs::create_dir_all(media_dir).map_err(io_error)?;
    let id = new_asset_id();
    let tmp = media_dir.join(format!("{id}.part"));
    let result = (|| {
        let mut input = fs::File::open(source).map_err(io_error)?;
        let mut output = fs::File::create(&tmp).map_err(io_error)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 1024 * 1024];
        let mut total: u64 = 0;
        loop {
            let read = input.read(&mut buffer).map_err(io_error)?;
            if read == 0 {
                break;
            }
            total += read as u64;
            if total > max_bytes {
                return Err(MediaError::new(
                    "too_large",
                    "the file grew past the size limit",
                ));
            }
            hasher.update(&buffer[..read]);
            output.write_all(&buffer[..read]).map_err(io_error)?;
        }
        output.flush().map_err(io_error)?;
        Ok((format!("{:x}", hasher.finalize()), total))
    })();
    match result {
        Ok((sha256, size)) => Ok(Staged {
            id,
            tmp,
            filename: sanitize_filename(
                &source
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ),
            sha256,
            size,
        }),
        Err(error) => {
            let _ = fs::remove_file(&tmp);
            Err(error)
        }
    }
}

/// Writes a text asset (always `.txt`) into the media directory.
pub fn stage_text(media_dir: &Path, name: &str, content: &str) -> Result<Staged, MediaError> {
    if content.trim().is_empty() {
        return Err(MediaError::new("empty_file", "the text is empty"));
    }
    if content.len() > MAX_TEXT_BYTES {
        return Err(MediaError::new("too_large", "the text is too long"));
    }
    fs::create_dir_all(media_dir).map_err(io_error)?;
    let id = new_asset_id();
    let tmp = media_dir.join(format!("{id}.part"));
    fs::write(&tmp, content.as_bytes()).map_err(io_error)?;
    let mut filename = sanitize_filename(name);
    if extension_of(&filename) != "txt" {
        filename.push_str(".txt");
    }
    Ok(Staged {
        id,
        tmp,
        filename,
        sha256: format!("{:x}", Sha256::digest(content.as_bytes())),
        size: content.len() as u64,
    })
}

/// An already stored asset with the same content, if any.
pub fn existing(vault: &VaultService, staged: &Staged) -> Result<Option<MediaAsset>, MediaError> {
    Ok(vault.find_media_by_sha(&staged.sha256)?)
}

/// Moves the staged file to `<id>.<ext>` and records it. `probe` comes from `probe(&staged.tmp, ..)`.
pub fn commit(
    vault: &VaultService,
    media_dir: &Path,
    staged: Staged,
    probed: Probe,
    source: &str,
    description: &str,
    tags: &[String],
) -> Result<MediaAsset, MediaError> {
    let extension = extension_of(&staged.filename);
    let stored = if extension.is_empty() {
        media_dir.join(&staged.id)
    } else {
        media_dir.join(format!("{}.{extension}", staged.id))
    };
    fs::rename(&staged.tmp, &stored).map_err(io_error)?;
    let asset = MediaAsset {
        id: staged.id.clone(),
        kind: kind_for(&staged.filename).to_owned(),
        filename: staged.filename.clone(),
        stored_path: stored.to_string_lossy().into_owned(),
        sha256: staged.sha256.clone(),
        size_bytes: staged.size as i64,
        mime: mime_for(&staged.filename).to_owned(),
        width: probed.width,
        height: probed.height,
        duration_ms: probed.duration_ms,
        description: clean_description(description),
        tags: normalize_tags(tags),
        source: if source == "ai" { "ai" } else { "user" }.to_owned(),
        created_at: crate::now_seconds() as i64,
    };
    if let Err(error) = vault.insert_media(&asset) {
        let _ = fs::remove_file(&stored);
        return Err(error.into());
    }
    Ok(asset)
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ImportOutcome {
    pub asset: MediaAsset,
    pub duplicate: bool,
}

/// Runs short database steps against the vault. Importing never holds the vault across the
/// file copy or an ffprobe run, so the UI stays responsive during large imports.
pub trait VaultAccess {
    fn run<T>(
        &self,
        action: impl FnOnce(&VaultService) -> Result<T, MediaError>,
    ) -> Result<T, MediaError>;
}

impl VaultAccess for VaultService {
    fn run<T>(
        &self,
        action: impl FnOnce(&VaultService) -> Result<T, MediaError>,
    ) -> Result<T, MediaError> {
        action(self)
    }
}

/// The vault inside the Tauri state; media works whether or not it is unlocked.
struct StateAccess<'a, 'b>(&'a State<'b, AppState>);

impl VaultAccess for StateAccess<'_, '_> {
    fn run<T>(
        &self,
        action: impl FnOnce(&VaultService) -> Result<T, MediaError>,
    ) -> Result<T, MediaError> {
        with_vault(self.0, action)
    }
}

/// Stage (copy + hash), dedupe, probe, commit.
pub fn import_path(
    access: &impl VaultAccess,
    media_dir: &Path,
    source: &Path,
    origin: &str,
    description: &str,
    tags: &[String],
) -> Result<ImportOutcome, MediaError> {
    let staged = stage_copy(media_dir, source, MAX_MEDIA_BYTES)?;
    finish_import(access, media_dir, staged, origin, description, tags)
}

fn finish_import(
    access: &impl VaultAccess,
    media_dir: &Path,
    staged: Staged,
    origin: &str,
    description: &str,
    tags: &[String],
) -> Result<ImportOutcome, MediaError> {
    if let Some(asset) = access.run(|vault| existing(vault, &staged))? {
        staged.discard();
        return Ok(ImportOutcome {
            asset,
            duplicate: true,
        });
    }
    let probed = probe(&staged.tmp, kind_for(&staged.filename));
    let asset =
        access.run(|vault| commit(vault, media_dir, staged, probed, origin, description, tags))?;
    Ok(ImportOutcome {
        asset,
        duplicate: false,
    })
}

/// Deletes the library copy of an asset (never the original file).
pub fn delete_asset(vault: &VaultService, media_dir: &Path, id: &str) -> Result<bool, MediaError> {
    let Some(asset) = vault.delete_media(id)? else {
        return Ok(false);
    };
    let stored = PathBuf::from(&asset.stored_path);
    // Only files inside the media directory are ever removed.
    if stored.parent() == Some(media_dir) {
        let _ = fs::remove_file(stored);
    }
    Ok(true)
}

/// `data:` URL of an image asset small enough for the webview; `None` for everything else.
pub fn thumbnail(asset: &MediaAsset, media_dir: &Path, max_bytes: u64) -> Option<String> {
    const SHOWN: [&str; 7] = [
        "image/png",
        "image/jpeg",
        "image/gif",
        "image/webp",
        "image/bmp",
        "image/avif",
        "image/svg+xml",
    ];
    let stored = Path::new(&asset.stored_path);
    if asset.kind != "image"
        || !SHOWN.contains(&asset.mime.as_str())
        || stored.parent() != Some(media_dir)
        || fs::metadata(stored).ok()?.len() > max_bytes
    {
        return None;
    }
    let bytes = fs::read(stored).ok()?;
    Some(format!(
        "data:{};base64,{}",
        asset.mime,
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

// ------------------------------------------------------- AI add requests

#[derive(Debug, Deserialize)]
pub struct PendingMedia {
    #[serde(default)]
    pub version: u32,
    #[serde(alias = "requestId")]
    pub request_id: String,
    pub kind: String,
    pub path: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(alias = "createdAt")]
    pub created_at: String,
}

impl PendingMedia {
    fn validated(self, expected_id: &str) -> Result<Self, String> {
        let bad = |reason: &str| Err(format!("invalid media request: {reason}"));
        if self.request_id != expected_id || !queue::valid_id(&self.request_id, PREFIX) {
            return bad("request id");
        }
        if self.kind != KIND || self.version > 1 {
            return bad("kind or version");
        }
        if self.path.is_empty() || self.path.len() > 1024 || self.path.contains('\0') {
            return bad("path");
        }
        if self.description.chars().count() > MAX_DESCRIPTION_CHARS || self.tags.len() > MAX_TAGS {
            return bad("description or tags");
        }
        Ok(self)
    }
}

pub fn read_pending(directory: &Path) -> Vec<PendingMedia> {
    let mut requests = queue::pending_files(directory, PREFIX)
        .into_iter()
        .filter_map(|(id, path)| {
            serde_json::from_str::<PendingMedia>(&fs::read_to_string(path).ok()?)
                .ok()?
                .validated(&id)
                .ok()
        })
        .collect::<Vec<_>>();
    requests.sort_by(|left, right| left.created_at.cmp(&right.created_at));
    requests
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaRequestView {
    pub request_id: String,
    pub path: String,
    pub filename: String,
    pub size_bytes: Option<u64>,
    /// Error code when the file can no longer be added (missing, directory, too large, empty).
    pub problem: Option<&'static str>,
    pub description: String,
    pub tags: Vec<String>,
    pub created_at: String,
}

fn view_of(request: PendingMedia) -> MediaRequestView {
    let path = Path::new(&request.path);
    let (size_bytes, problem) = match validate_source(path, MAX_MEDIA_BYTES) {
        Ok(size) => (Some(size), None),
        Err(error) => (
            fs::metadata(path).ok().map(|meta| meta.len()),
            Some(error.code),
        ),
    };
    MediaRequestView {
        filename: sanitize_filename(
            &path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        ),
        request_id: request.request_id,
        path: request.path,
        size_bytes,
        problem,
        description: request.description,
        tags: request.tags,
        created_at: request.created_at,
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MediaResult<'a> {
    request_id: &'a str,
    /// `added` | `declined` | `failed`
    status: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    asset_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duplicate: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<serde_json::Value>,
    finished_at: String,
}

// -------------------------------------------------------------- commands

fn media_directory(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("media"))
}

fn queue_directories(app: &AppHandle) -> Result<(PathBuf, PathBuf), String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    Ok((data.join("mcp-pending"), data.join("mcp-results")))
}

/// Runs `action` with the vault; the media database does not need the vault to be unlocked.
fn with_vault<T>(
    state: &State<'_, AppState>,
    action: impl FnOnce(&VaultService) -> Result<T, MediaError>,
) -> Result<T, MediaError> {
    let guard = state
        .vault
        .lock()
        .map_err(|_| MediaError::new("storage", "vault state is unavailable"))?;
    let vault = guard
        .as_ref()
        .ok_or_else(|| MediaError::new("storage", "vault unavailable"))?;
    action(vault)
}

#[derive(Debug, Serialize)]
pub struct MediaListing {
    pub assets: Vec<MediaAsset>,
    pub count: usize,
    pub usage_bytes: i64,
    pub ffprobe: bool,
}

#[tauri::command]
pub fn media_list(state: State<'_, AppState>) -> Result<MediaListing, String> {
    let assets = with_vault(&state, |vault| Ok(vault.list_media()?)).map_err(|e| e.to_string())?;
    Ok(MediaListing {
        count: assets.len(),
        usage_bytes: assets.iter().map(|asset| asset.size_bytes).sum(),
        ffprobe: ffprobe_available(),
        assets,
    })
}

#[tauri::command]
pub fn media_choose_files() -> Option<Vec<String>> {
    rfd::FileDialog::new().pick_files().map(|paths| {
        paths
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect()
    })
}

#[derive(Debug, Serialize)]
pub struct ImportItem {
    pub path: String,
    pub ok: bool,
    pub id: Option<String>,
    pub duplicate: bool,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn media_import(
    paths: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<ImportItem>, String> {
    if paths.len() > MAX_IMPORT_PATHS {
        return Err("too many files at once".to_owned());
    }
    let directory = media_directory(&app)?;
    let access = StateAccess(&state);
    Ok(paths
        .into_iter()
        .map(
            |path| match import_path(&access, &directory, Path::new(&path), "user", "", &[]) {
                Ok(outcome) => ImportItem {
                    path,
                    ok: true,
                    id: Some(outcome.asset.id),
                    duplicate: outcome.duplicate,
                    error: None,
                },
                Err(error) => ImportItem {
                    path,
                    ok: false,
                    id: None,
                    duplicate: false,
                    error: Some(error.to_string()),
                },
            },
        )
        .collect())
}

#[tauri::command]
pub async fn media_create_text(
    name: String,
    content: String,
    description: String,
    tags: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<MediaAsset, String> {
    let directory = media_directory(&app)?;
    let staged = stage_text(&directory, &name, &content).map_err(|e| e.to_string())?;
    finish_import(
        &StateAccess(&state),
        &directory,
        staged,
        "user",
        &description,
        &tags,
    )
    .map(|outcome| outcome.asset)
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn media_update(
    id: String,
    description: String,
    tags: Vec<String>,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    with_vault(&state, |vault| {
        Ok(vault.update_media_meta(
            &id,
            &clean_description(&description),
            &normalize_tags(&tags),
        )?)
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn media_delete(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let directory = media_directory(&app)?;
    with_vault(&state, |vault| delete_asset(vault, &directory, &id)).map_err(|e| e.to_string())
}

fn asset_of(state: &State<'_, AppState>, id: &str) -> Result<MediaAsset, String> {
    with_vault(state, |vault| Ok(vault.get_media(id)?))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "media asset not found".to_owned())
}

/// Copies the stored file's absolute path (not secret) to the clipboard.
#[tauri::command]
pub fn media_copy_path(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let asset = asset_of(&state, &id)?;
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(asset.stored_path))
        .map_err(|error| error.to_string())
}

/// Shows the stored copy in the system file manager; only library files can be revealed.
#[tauri::command]
pub fn media_reveal(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let asset = asset_of(&state, &id)?;
    let path = PathBuf::from(&asset.stored_path);
    if !path.is_file() {
        return Err("the stored file is missing".to_owned());
    }
    #[cfg(windows)]
    let result = Command::new("explorer")
        .arg(format!("/select,{}", path.display()))
        .spawn()
        .map(|_| ());
    #[cfg(target_os = "macos")]
    let result = Command::new("open")
        .arg("-R")
        .arg(&path)
        .spawn()
        .map(|_| ());
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let result = open::that(path.parent().unwrap_or(&path));
    result.map_err(|error| error.to_string())
}

/// Small-image preview as a `data:` URL (images up to 10 MB). The asset protocol would need a
/// Cargo feature plus a scoped config entry the project does not have, so previews go through IPC.
#[tauri::command]
pub fn media_thumbnail(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let asset = asset_of(&state, &id)?;
    Ok(thumbnail(
        &asset,
        &media_directory(&app)?,
        MAX_THUMBNAIL_BYTES,
    ))
}

/// AI add requests waiting for confirmation; works while the vault is locked.
#[tauri::command]
pub fn mcp_pending_media_requests(app: AppHandle) -> Result<Vec<MediaRequestView>, String> {
    let (pending, _) = queue_directories(&app)?;
    Ok(read_pending(&pending).into_iter().map(view_of).collect())
}

fn write_result(app: &AppHandle, result: &MediaResult<'_>) {
    if let Ok((_, results)) = queue_directories(app) {
        let _ = queue::write_result(&results, result.request_id, PREFIX, result);
    }
}

/// Accepts (copies the file into the library with source `ai`) or declines an AI add request.
#[tauri::command]
pub async fn mcp_media_decision(
    request_id: String,
    accept: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if !queue::valid_id(&request_id, PREFIX) {
        return Err("invalid media request id".to_owned());
    }
    let (pending, _) = queue_directories(&app)?;
    let path = pending.join(format!("{request_id}.json"));
    let request = serde_json::from_str::<PendingMedia>(
        &fs::read_to_string(&path).map_err(|_| "media request not found".to_owned())?,
    )
    .map_err(|_| "media request is invalid".to_owned())?
    .validated(&request_id)?;
    let finished_at = chrono::Utc::now().to_rfc3339();
    if !accept {
        write_result(
            &app,
            &MediaResult {
                request_id: &request_id,
                status: "declined",
                asset_id: None,
                duplicate: None,
                error: None,
                finished_at,
            },
        );
        return fs::remove_file(&path).map_err(|error| error.to_string());
    }
    let directory = media_directory(&app)?;
    let outcome = import_path(
        &StateAccess(&state),
        &directory,
        Path::new(&request.path),
        "ai",
        &request.description,
        &request.tags,
    );
    match &outcome {
        Ok(done) => write_result(
            &app,
            &MediaResult {
                request_id: &request_id,
                status: "added",
                asset_id: Some(&done.asset.id),
                duplicate: Some(done.duplicate),
                error: None,
                finished_at,
            },
        ),
        Err(error) => write_result(
            &app,
            &MediaResult {
                request_id: &request_id,
                status: "failed",
                asset_id: None,
                duplicate: None,
                error: Some(serde_json::json!({ "code": error.code })),
                finished_at,
            },
        ),
    }
    fs::remove_file(&path).map_err(|error| error.to_string())?;
    // Audit is best effort: it needs an unlocked vault and the file is already in the library.
    if let Ok(done) = &outcome {
        let _ = with_vault(&state, |vault| {
            let _ = vault.record_action(
                "media_added",
                None,
                &serde_json::json!({"source": "mcp", "assetId": done.asset.id}).to_string(),
            );
            Ok(())
        });
    }
    outcome.map(|_| ()).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrethub_storage::Database;

    fn vault(dir: &Path) -> VaultService {
        VaultService::open(Database::open(dir.join("vault.sqlite")).unwrap()).unwrap()
    }

    /// Minimal PNG header: signature, IHDR length+type, width 2, height 3.
    fn tiny_png() -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend([0, 0, 0, 13, b'I', b'H', b'D', b'R']);
        bytes.extend([0, 0, 0, 2, 0, 0, 0, 3, 8, 6, 0, 0, 0]);
        bytes.extend([0; 4]);
        bytes
    }

    fn run_import(
        vault: &VaultService,
        media_dir: &Path,
        source: &Path,
        origin: &str,
    ) -> Result<ImportOutcome, MediaError> {
        import_path(
            vault,
            media_dir,
            source,
            origin,
            "  a cat  ",
            &["Cat".into(), "cat".into(), "bad,tag".into(), " ".into()],
        )
    }

    #[test]
    fn detects_kind_and_mime_from_the_extension() {
        for (name, kind, mime) in [
            ("a.PNG", "image", "image/png"),
            ("clip.mp4", "video", "video/mp4"),
            ("voice.mp3", "audio", "audio/mpeg"),
            ("notes.txt", "text", "text/plain"),
            ("script.md", "text", "text/markdown"),
            ("archive.zip", "other", "application/octet-stream"),
            ("noext", "other", "application/octet-stream"),
            ("weird.ex e", "other", "application/octet-stream"),
        ] {
            assert_eq!((kind_for(name), mime_for(name)), (kind, mime), "{name}");
        }
    }

    #[test]
    fn sanitizes_display_names() {
        assert_eq!(sanitize_filename("C:\\a\\b\\pic.png"), "pic.png");
        assert_eq!(sanitize_filename("../../etc/pass\nwd"), "passwd");
        assert_eq!(sanitize_filename("   "), "untitled");
        assert_eq!(sanitize_filename(&"x".repeat(300)).chars().count(), 120);
    }

    #[test]
    fn parses_ffprobe_output() {
        let probe = parse_ffprobe(
            r#"{"streams":[{"codec_type":"audio"},{"codec_type":"video","width":1920,"height":1080}],"format":{"duration":"12.3456"}}"#,
        );
        assert_eq!(
            probe,
            Probe {
                width: Some(1920),
                height: Some(1080),
                duration_ms: Some(12346)
            }
        );
        let audio =
            parse_ffprobe(r#"{"streams":[{"codec_type":"audio"}],"format":{"duration":3}}"#);
        assert_eq!((audio.width, audio.duration_ms), (None, Some(3000)));
        assert_eq!(parse_ffprobe("not json"), Probe::default());
        assert_eq!(
            parse_ffprobe(r#"{"format":{"duration":"N/A"}}"#),
            Probe::default()
        );
    }

    #[test]
    fn imports_copies_dedupes_and_reads_image_dimensions() {
        let dir = tempfile::tempdir().unwrap();
        let media = dir.path().join("media");
        let vault = vault(dir.path());
        let source = dir.path().join("Cat Photo.PNG");
        fs::write(&source, tiny_png()).unwrap();

        let first = run_import(&vault, &media, &source, "user").unwrap();
        assert!(!first.duplicate);
        let asset = &first.asset;
        assert_eq!(
            (asset.kind.as_str(), asset.mime.as_str()),
            ("image", "image/png")
        );
        assert_eq!(
            (asset.width, asset.height, asset.duration_ms),
            (Some(2), Some(3), None)
        );
        assert_eq!(asset.filename, "Cat Photo.PNG");
        assert_eq!(asset.description, "a cat");
        assert_eq!(asset.tags, ["Cat"]);
        assert_eq!(asset.source, "user");
        assert!(Path::new(&asset.stored_path).starts_with(&media));
        assert!(asset.stored_path.ends_with(".png"));
        assert_eq!(fs::read(&asset.stored_path).unwrap(), tiny_png());
        assert!(source.exists(), "the original stays untouched");

        // Same content again: no second copy, same row, even from a different name and origin.
        let other = dir.path().join("renamed.png");
        fs::copy(&source, &other).unwrap();
        let again = run_import(&vault, &media, &other, "ai").unwrap();
        assert!(again.duplicate);
        assert_eq!(again.asset.id, asset.id);
        assert_eq!(
            fs::read_dir(&media).unwrap().count(),
            1,
            "no temp or duplicate files left"
        );
        assert_eq!(vault.list_media().unwrap().len(), 1);
    }

    #[test]
    fn rejects_missing_directory_empty_and_oversize_sources() {
        let dir = tempfile::tempdir().unwrap();
        let media = dir.path().join("media");
        let vault = vault(dir.path());
        let code = |result: Result<ImportOutcome, MediaError>| result.unwrap_err().code;
        assert_eq!(
            code(run_import(
                &vault,
                &media,
                &dir.path().join("nope.png"),
                "user"
            )),
            "not_found"
        );
        assert_eq!(
            code(run_import(&vault, &media, dir.path(), "user")),
            "not_a_file"
        );
        let empty = dir.path().join("empty.txt");
        fs::write(&empty, b"").unwrap();
        assert_eq!(
            code(run_import(&vault, &media, &empty, "user")),
            "empty_file"
        );
        // The size cap is a parameter so the test needs no huge file.
        let big = dir.path().join("big.bin");
        fs::write(&big, vec![1u8; 2048]).unwrap();
        assert_eq!(validate_source(&big, 4096), Ok(2048));
        assert_eq!(validate_source(&big, 1024).unwrap_err().code, "too_large");
        assert_eq!(
            stage_copy(&media, &big, 1024).unwrap_err().code,
            "too_large"
        );
        assert!(fs::read_dir(&media)
            .map(|mut read| read.next().is_none())
            .unwrap_or(true));
        assert!(vault.list_media().unwrap().is_empty());
        assert_eq!(MAX_MEDIA_BYTES, 2 * 1024 * 1024 * 1024);
    }

    #[test]
    fn creates_text_assets_as_txt_and_dedupes_by_content() {
        let dir = tempfile::tempdir().unwrap();
        let media = dir.path().join("media");
        let vault = vault(dir.path());
        let staged = stage_text(&media, "Shot list", "scene 1\nscene 2").unwrap();
        let first = finish_import(&vault, &media, staged, "user", "", &[]).unwrap();
        assert_eq!(
            (first.asset.kind.as_str(), first.asset.filename.as_str()),
            ("text", "Shot list.txt")
        );
        assert_eq!(
            fs::read_to_string(&first.asset.stored_path).unwrap(),
            "scene 1\nscene 2"
        );
        let staged = stage_text(&media, "other name.txt", "scene 1\nscene 2").unwrap();
        assert!(
            finish_import(&vault, &media, staged, "user", "", &[])
                .unwrap()
                .duplicate
        );
        assert_eq!(
            stage_text(&media, "x", "  \n").unwrap_err().code,
            "empty_file"
        );
    }

    #[test]
    fn deleting_removes_only_the_library_copy() {
        let dir = tempfile::tempdir().unwrap();
        let media = dir.path().join("media");
        let vault = vault(dir.path());
        let source = dir.path().join("a.png");
        fs::write(&source, tiny_png()).unwrap();
        let asset = run_import(&vault, &media, &source, "user").unwrap().asset;
        assert!(delete_asset(&vault, &media, &asset.id).unwrap());
        assert!(!Path::new(&asset.stored_path).exists());
        assert!(source.exists());
        assert!(!delete_asset(&vault, &media, &asset.id).unwrap());

        // A row pointing outside the media directory never deletes that file.
        let outside = dir.path().join("keep.png");
        fs::write(&outside, b"keep").unwrap();
        let mut rogue = asset.clone();
        rogue.id = "asset-rogue".into();
        rogue.sha256 = "ff".repeat(32);
        rogue.stored_path = outside.to_string_lossy().into_owned();
        vault.insert_media(&rogue).unwrap();
        assert!(delete_asset(&vault, &media, "asset-rogue").unwrap());
        assert!(outside.exists());
    }

    #[test]
    fn thumbnails_are_data_urls_for_small_library_images_only() {
        let dir = tempfile::tempdir().unwrap();
        let media = dir.path().join("media");
        let vault = vault(dir.path());
        let source = dir.path().join("a.png");
        fs::write(&source, tiny_png()).unwrap();
        let asset = run_import(&vault, &media, &source, "user").unwrap().asset;
        let url = thumbnail(&asset, &media, MAX_THUMBNAIL_BYTES).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        assert!(thumbnail(&asset, &media, 4).is_none(), "over the cap");
        let mut text = asset.clone();
        text.kind = "text".into();
        assert!(thumbnail(&text, &media, MAX_THUMBNAIL_BYTES).is_none());
        let mut outside = asset.clone();
        outside.stored_path = source.to_string_lossy().into_owned();
        assert!(thumbnail(&outside, &media, MAX_THUMBNAIL_BYTES).is_none());
    }

    #[test]
    fn pending_requests_are_validated_and_isolated_from_other_queues() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("pic.png");
        fs::write(&target, tiny_png()).unwrap();
        let write = |name: &str, body: String| fs::write(dir.path().join(name), body).unwrap();
        let file = |id: &str, path: &str, extra: &str| {
            format!(
                r#"{{"version":1,"requestId":"{id}","kind":"add_media","path":{},{extra}"createdAt":"2026-10-28T00:00:00Z"}}"#,
                serde_json::to_string(path).unwrap()
            )
        };
        write(
            "media-ok.json",
            file(
                "media-ok",
                &target.to_string_lossy(),
                r#""description":"d","tags":["t"],"#,
            ),
        );
        write(
            "media-gone.json",
            file("media-gone", "C:/definitely/not/here.png", ""),
        );
        write("media-mismatch.json", file("media-other", "C:/x.png", ""));
        write("media-nopath.json", file("media-nopath", "", ""));
        write("sync-1.json", "{}".into());
        let found = read_pending(dir.path());
        let mut ids: Vec<_> = found.iter().map(|item| item.request_id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, ["media-gone", "media-ok"]);
        let views: Vec<_> = found.into_iter().map(view_of).collect();
        let ok = views
            .iter()
            .find(|view| view.request_id == "media-ok")
            .unwrap();
        assert_eq!(
            (ok.filename.as_str(), ok.problem, ok.size_bytes),
            ("pic.png", None, Some(tiny_png().len() as u64))
        );
        let gone = views
            .iter()
            .find(|view| view.request_id == "media-gone")
            .unwrap();
        assert_eq!((gone.problem, gone.size_bytes), (Some("not_found"), None));
        assert!(crate::sync::read_pending(dir.path()).is_empty());
        assert!(crate::caps::read_pending(dir.path()).is_empty());
        assert!(!crate::plan::valid_request_id("media-ok"));
    }
}
