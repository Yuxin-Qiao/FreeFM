use crate::cli::Cli;
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
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
