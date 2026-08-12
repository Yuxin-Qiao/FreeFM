use crate::cli::Cli;
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    pub fn from_cli(cli: &Cli) -> Self {
        let root = cli
            .data_dir
            .clone()
            .or_else(|| env::var_os("FREEFM_HOME").map(PathBuf::from))
            .unwrap_or_else(|| {
                env::var_os("HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".freefm")
            });
        Self { root }
    }
    pub(crate) fn session(&self) -> PathBuf {
        self.root.join("session.json")
    }
    pub(crate) fn state(&self) -> PathBuf {
        self.root.join("state.json")
    }
    pub(crate) fn lock(&self) -> PathBuf {
        self.root.join("sync.lock")
    }
    pub(crate) fn trusted(&self) -> PathBuf {
        self.root.join("trusted.json")
    }
    pub(crate) fn external_mappings(&self) -> PathBuf {
        self.root.join("external-mappings.json")
    }
}

pub(crate) struct SyncLock {
    _file: File,
}

impl SyncLock {
    pub(crate) fn acquire(paths: &Paths) -> AppResult<Self> {
        restrict_dir(&paths.root)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(paths.lock())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
            let result = unsafe {
                libc::flock(
                    std::os::fd::AsRawFd::as_raw_fd(&file),
                    libc::LOCK_EX | libc::LOCK_NB,
                )
            };
            if result != 0 {
                return Err(AppError::ConcurrentSync);
            }
        }
        Ok(Self { _file: file })
    }
}

impl Drop for SyncLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            let _ =
                unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&self._file), libc::LOCK_UN) };
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StoredCookie {
    pub(crate) name: String,
    pub(crate) value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SessionFile {
    pub(crate) cookies: Vec<StoredCookie>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct StateFile {
    pub(crate) playlist_id: Option<String>,
    pub(crate) last_sync_at: Option<u64>,
    /// Song IDs FreeFM itself appended successfully. Used only to scope a
    /// future safe repair; never to delete user-added tracks.
    #[serde(default)]
    pub(crate) added_song_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TrustedMapping {
    pub(crate) target_id: String,
    pub(crate) approved_at: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct TrustedStore {
    pub(crate) mappings: HashMap<String, TrustedMapping>,
}

impl TrustedStore {
    pub(crate) fn approve(&mut self, original_id: &str, target_id: &str) {
        self.mappings.insert(
            original_id.to_string(),
            TrustedMapping {
                target_id: target_id.to_string(),
                approved_at: now_seconds(),
            },
        );
    }
}

/// Explicit source-to-external-target approvals. This is deliberately kept
/// separate from the NetEase trusted store: an external mapping proves only
/// that the user selected a target item, never that the item is free on
/// NetEase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ExternalMapping {
    #[serde(default)]
    pub(crate) source_kind: String,
    #[serde(default)]
    pub(crate) source_playlist_id: String,
    #[serde(default)]
    pub(crate) source_storefront: Option<String>,
    pub(crate) target_kind: String,
    #[serde(default)]
    pub(crate) target_playlist_id: String,
    #[serde(default)]
    pub(crate) target_storefront: Option<String>,
    pub(crate) target_id: String,
    pub(crate) approved_at: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct ExternalMappingStore {
    pub(crate) mappings: HashMap<String, ExternalMapping>,
}

impl ExternalMappingStore {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn approve(
        &mut self,
        source_key: &str,
        source_kind: &str,
        source_playlist_id: &str,
        source_storefront: Option<&str>,
        target_kind: &str,
        target_playlist_id: &str,
        target_storefront: Option<&str>,
        target_id: &str,
    ) {
        self.mappings.insert(
            source_key.to_string(),
            ExternalMapping {
                source_kind: source_kind.to_string(),
                source_playlist_id: source_playlist_id.to_string(),
                source_storefront: source_storefront.map(str::to_string),
                target_kind: target_kind.to_string(),
                target_playlist_id: target_playlist_id.to_string(),
                target_storefront: target_storefront.map(str::to_string),
                target_id: target_id.to_string(),
                approved_at: now_seconds(),
            },
        );
    }
}

pub(crate) fn load_session(paths: &Paths) -> AppResult<Option<SessionFile>> {
    if !paths.session().exists() {
        return Ok(None);
    }
    let bytes = fs::read(paths.session())?;
    let session = serde_json::from_slice::<SessionFile>(&bytes)
        .map_err(|_| AppError::StateCorrupt("session.json 不是有效的会话文件".to_string()))?;
    if session
        .cookies
        .iter()
        .all(|cookie| cookie.name != "MUSIC_U")
    {
        return Err(AppError::StateCorrupt(
            "session.json 缺少 MUSIC_U".to_string(),
        ));
    }
    Ok(Some(session))
}

pub(crate) fn load_state(paths: &Paths) -> AppResult<(StateFile, bool)> {
    if !paths.state().exists() {
        return Ok((StateFile::default(), false));
    }
    let bytes = fs::read(paths.state())?;
    match serde_json::from_slice::<StateFile>(&bytes) {
        Ok(state) => Ok((state, false)),
        Err(_) => Ok((StateFile::default(), true)),
    }
}

pub(crate) fn load_trusted(paths: &Paths) -> AppResult<(TrustedStore, bool)> {
    if !paths.trusted().exists() {
        return Ok((TrustedStore::default(), false));
    }
    let bytes = fs::read(paths.trusted())?;
    match serde_json::from_slice::<TrustedStore>(&bytes) {
        Ok(store) => Ok((store, false)),
        // Fail closed to no trusted mappings rather than deleting user data.
        Err(_) => Ok((TrustedStore::default(), true)),
    }
}

pub(crate) fn save_trusted(paths: &Paths, store: &TrustedStore) -> AppResult<()> {
    write_private_json(&paths.trusted(), store)
}

pub(crate) fn load_external_mappings(paths: &Paths) -> AppResult<(ExternalMappingStore, bool)> {
    if !paths.external_mappings().exists() {
        return Ok((ExternalMappingStore::default(), false));
    }
    let bytes = fs::read(paths.external_mappings())?;
    match serde_json::from_slice::<ExternalMappingStore>(&bytes) {
        Ok(store) => Ok((store, false)),
        // Fail closed to no external mappings rather than trusting malformed
        // local data for a remote playlist write.
        Err(_) => Ok((ExternalMappingStore::default(), true)),
    }
}

pub(crate) fn save_external_mappings(paths: &Paths, store: &ExternalMappingStore) -> AppResult<()> {
    write_private_json(&paths.external_mappings(), store)
}

pub(crate) fn restrict_dir(path: &Path) -> AppResult<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub(crate) fn write_private_json(path: &Path, value: &impl Serialize) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Io(io::Error::other("state path has no parent")))?;
    restrict_dir(parent)?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(
        ".{}.tmp-{}-{nonce}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("state"),
        std::process::id()
    ));
    let bytes = serde_json::to_vec_pretty(value)?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temporary, path)?;
    Ok(())
}

pub(crate) fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
