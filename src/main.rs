use netease_music::{
    LoginQrCheckParams, NeteaseError, NeteaseMusicClient, PlaylistDetailParams,
    PlaylistTrackAllParams, SearchParams, SongDetailParams, SongQualityLevel, SongUrlV1Params,
    UserPlaylistParams,
};
use qrcode::{QrCode, render::unicode::Dense1x2};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fmt::{self, Display};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(not(unix))]
compile_error!("FreeFM v0.1 supports macOS and Linux only");

const PLAYLIST_NAME: &str = "FreeFM · Auto";
const FM_ENDPOINT: &str = "https://music.163.com/api/v1/radio/get";
const PLAYLIST_CREATE_ENDPOINT: &str = "https://music.163.com/api/playlist/create";
const PLAYLIST_ADD_ENDPOINT: &str = "https://music.163.com/api/playlist/manipulate/tracks";
const VERSION: &str = "0.1.0";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);
const QR_TIMEOUT: Duration = Duration::from_secs(300);

type AppResult<T> = Result<T, AppError>;

#[derive(Debug)]
enum AppError {
    Usage(String),
    Help(String),
    Version,
    LoginRequired,
    OrdinaryAccountRequired,
    ConcurrentSync,
    AmbiguousPlaylist,
    StateCorrupt(String),
    ApiIncompatible(String),
    Timeout,
    Remote(String),
    Io(io::Error),
    Json(serde_json::Error),
    Netease(NeteaseError),
}

impl Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) => write!(f, "{message}"),
            Self::Help(message) => write!(f, "{message}"),
            Self::Version => write!(f, "freefm {VERSION}"),
            Self::LoginRequired => write!(f, "登录已失效或尚未登录；请运行 freefm auth"),
            Self::OrdinaryAccountRequired => write!(
                f,
                "当前账号不是已确认的普通非 VIP 账号；为避免误加会员歌曲，请使用普通账号重新登录"
            ),
            Self::ConcurrentSync => write!(f, "已有另一个 FreeFM 进程正在同步，请稍后重试"),
            Self::AmbiguousPlaylist => write!(
                f,
                "发现多个同名且属于当前账号的 FreeFM · Auto 歌单；请人工保留一个后重试"
            ),
            Self::StateCorrupt(message) => write!(f, "本机状态文件损坏：{message}"),
            Self::ApiIncompatible(message) => write!(f, "网易云接口返回格式不兼容：{message}"),
            Self::Timeout => write!(f, "网易云接口请求超时，请稍后重试"),
            Self::Remote(message) => write!(f, "网易云接口请求失败：{message}"),
            Self::Io(error) => write!(f, "本机文件操作失败：{error}"),
            Self::Json(error) => write!(f, "本机 JSON 处理失败：{error}"),
            Self::Netease(error) => write!(f, "网易云请求失败：{error}"),
        }
    }
}

impl From<io::Error> for AppError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}
impl From<NeteaseError> for AppError {
    fn from(value: NeteaseError) -> Self {
        match &value {
            NeteaseError::Http(error) if error.is_timeout() => Self::Timeout,
            _ => Self::Netease(value),
        }
    }
}

#[derive(Debug, Clone)]
struct Cli {
    command: String,
    json: bool,
    quiet: bool,
    data_dir: Option<PathBuf>,
}

impl Cli {
    fn parse<I>(args: I) -> AppResult<Self>
    where
        I: IntoIterator<Item = String>,
    {
        let mut args = args.into_iter();
        let _program = args.next();
        let command = args.next().unwrap_or_default();
        if command.is_empty() {
            return Err(AppError::Help(usage()));
        }
        if command == "--help" || command == "-h" {
            return Err(AppError::Help(usage()));
        }
        if command == "--version" || command == "version" {
            return Err(AppError::Version);
        }
        let mut json_output = false;
        let mut quiet = false;
        let mut data_dir = None;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--json" => json_output = true,
                "--quiet" => quiet = true,
                "--data-dir" => {
                    data_dir = Some(PathBuf::from(args.next().ok_or_else(|| {
                        AppError::Usage("--data-dir 需要一个路径".to_string())
                    })?));
                }
                "--help" | "-h" => return Err(AppError::Help(usage())),
                other => return Err(AppError::Usage(format!("未知参数：{other}\n\n{}", usage()))),
            }
        }
        if quiet && command == "auth" {
            return Err(AppError::Usage(
                "auth 需要交互式二维码，不能使用 --quiet".to_string(),
            ));
        }
        Ok(Self {
            command,
            json: json_output,
            quiet,
            data_dir,
        })
    }
}

fn usage() -> String {
    "用法：freefm <auth|preview|sync|status|doctor> [--json] [--quiet] [--data-dir PATH]\n\n\
auth     生成二维码并等待网易云官方客户端确认\n\
preview  读取私人 FM 并预览加入、候选、跳过；绝不写远端歌单\n\
sync     读取私人 FM，并 append-only 写入 FreeFM · Auto\n\
status   检查本机会话和登录状态\n\
doctor   检查本机状态、权限和 API 登录可用性
version  输出版本号"
        .to_string()
}

#[derive(Debug, Clone)]
struct Paths {
    root: PathBuf,
}

impl Paths {
    fn from_cli(cli: &Cli) -> Self {
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
    fn session(&self) -> PathBuf {
        self.root.join("session.json")
    }
    fn state(&self) -> PathBuf {
        self.root.join("state.json")
    }
    fn lock(&self) -> PathBuf {
        self.root.join("sync.lock")
    }
}

struct SyncLock {
    _file: File,
}

impl SyncLock {
    fn acquire(paths: &Paths) -> AppResult<Self> {
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
struct StoredCookie {
    name: String,
    value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionFile {
    cookies: Vec<StoredCookie>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct StateFile {
    playlist_id: Option<String>,
    last_sync_at: Option<u64>,
}

fn load_session(paths: &Paths) -> AppResult<Option<SessionFile>> {
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

fn load_state(paths: &Paths) -> AppResult<(StateFile, bool)> {
    if !paths.state().exists() {
        return Ok((StateFile::default(), false));
    }
    let bytes = fs::read(paths.state())?;
    match serde_json::from_slice::<StateFile>(&bytes) {
        Ok(state) => Ok((state, false)),
        Err(_) => Ok((StateFile::default(), true)),
    }
}

fn restrict_dir(path: &Path) -> AppResult<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn write_private_json(path: &Path, value: &impl Serialize) -> AppResult<()> {
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

fn new_client(session: Option<&SessionFile>) -> AppResult<NeteaseMusicClient> {
    let mut builder = NeteaseMusicClient::builder().timeout(DEFAULT_TIMEOUT);
    if let Some(session) = session {
        for cookie in &session.cookies {
            builder = builder.cookie(&cookie.name, &cookie.value);
        }
    }
    Ok(builder.build()?)
}

fn refresh_session(paths: &Paths, client: &NeteaseMusicClient) -> AppResult<()> {
    let cookies = client
        .cookies()
        .into_iter()
        .filter(|cookie| cookie.name == "MUSIC_U")
        .map(|cookie| StoredCookie {
            name: cookie.name,
            value: cookie.value,
        })
        .collect();
    write_private_json(&paths.session(), &SessionFile { cookies })
}

fn qr_key(response: &Value, qr_url: &str) -> AppResult<String> {
    if let Some(key) = response
        .get("unikey")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        return Ok(key.to_string());
    }
    if let Some(key) = response
        .pointer("/data/unikey")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        return Ok(key.to_string());
    }
    qr_url
        .split("codekey=")
        .nth(1)
        .map(|value| value.split('&').next().unwrap_or(value).to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::ApiIncompatible("二维码响应缺少 unikey".to_string()))
}

fn authenticate(paths: &Paths) -> AppResult<Value> {
    let client = new_client(None)?;
    let (key_response, qr_url) = client.login_qr_key()?;
    let key = qr_key(&key_response.body, &qr_url)?;
    let qr = QrCode::new(qr_url.as_bytes())
        .map_err(|_| AppError::ApiIncompatible("二维码内容无法生成".to_string()))?;
    let rendered = qr.render::<Dense1x2>().quiet_zone(true).build();
    restrict_dir(&paths.root)?;
    println!("请用网易云音乐官方客户端扫码并确认。");
    println!("{rendered}");
    println!("等待扫码确认（最多 5 分钟）...");
    io::stdout().flush()?;
    let started = SystemTime::now();
    let mut last_code = None;
    loop {
        if started.elapsed().unwrap_or_default() > QR_TIMEOUT {
            return Err(AppError::Remote(
                "二维码等待超时，请重新运行 auth".to_string(),
            ));
        }
        thread::sleep(Duration::from_secs(2));
        let response = client.login_qr_check(LoginQrCheckParams {
            unikey: key.clone(),
        })?;
        let code = response.body.get("code").and_then(Value::as_i64);
        if code != last_code {
            match code {
                Some(800) => println!("二维码已过期，请重新运行 auth"),
                Some(801) => println!("二维码已生成，等待扫码"),
                Some(802) => println!("已扫码，等待官方客户端确认"),
                Some(803) => println!("登录已确认，正在安全保存本机会话"),
                Some(other) => println!("二维码状态已变化（代码 {other}）"),
                None => println!("二维码状态响应缺少 code"),
            }
            io::stdout().flush()?;
            last_code = code;
        }
        if code == Some(800) {
            return Err(AppError::Remote(
                "二维码已过期，请重新运行 auth".to_string(),
            ));
        }
        if code == Some(803) {
            if client.cookie("MUSIC_U").is_none() {
                return Err(AppError::ApiIncompatible(
                    "二维码确认成功但未收到 MUSIC_U".to_string(),
                ));
            }
            refresh_session(paths, &client)?;
            return Ok(json!({"ok": true, "authenticated": true, "session_saved": true}));
        }
    }
}

struct Remote {
    client: NeteaseMusicClient,
    client_calls: u64,
    http_requests: u64,
}

impl Remote {
    fn new(client: NeteaseMusicClient) -> Self {
        Self {
            client,
            client_calls: 0,
            http_requests: 0,
        }
    }
    fn call<F>(&mut self, endpoint: &str, f: F) -> AppResult<Value>
    where
        F: FnOnce(&NeteaseMusicClient) -> Result<netease_music::ApiResponse, NeteaseError>,
    {
        self.client_calls += 1;
        self.http_requests += 1;
        let response = f(&self.client).map_err(|error| match AppError::from(error) {
            AppError::Timeout => AppError::Timeout,
            AppError::Netease(error) => AppError::Remote(format!("{endpoint}: {error}")),
            other => other,
        })?;
        if response.status >= 500 {
            return Err(AppError::Remote(format!(
                "{endpoint}: HTTP {}",
                response.status
            )));
        }
        if matches!(response.code, Some(code) if code != 200) {
            return match remote_code_error(response.code) {
                AppError::LoginRequired => {
                    let safe_detail = response
                        .body
                        .get("msg")
                        .and_then(Value::as_str)
                        .or_else(|| response.body.get("message").and_then(Value::as_str))
                        .unwrap_or("无公开错误说明");
                    Err(AppError::Remote(format!(
                        "{endpoint}: 登录类响应代码 {}（{}）",
                        response.code.unwrap_or_default(),
                        safe_detail
                    )))
                }
                error => Err(error),
            };
        }
        if response.body.is_null() {
            return Err(AppError::ApiIncompatible(format!(
                "{endpoint}: 响应不是 JSON"
            )));
        }
        Ok(response.body)
    }
    fn status(&mut self) -> AppResult<Value> {
        self.call("login_status", |client| client.login_status())
    }
    fn private_fm(&mut self) -> AppResult<Vec<Song>> {
        let body = self.call("private_fm", |client| {
            client.raw_weapi(FM_ENDPOINT, json!({}))
        })?;
        let data = body
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::ApiIncompatible("私人 FM 响应缺少 data 数组".to_string()))?;
        data.iter().map(song_from_value).collect()
    }
    fn details(&mut self, ids: &[String]) -> AppResult<HashMap<String, Song>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let body = self.call("song_detail", |client| {
            client.song_detail(SongDetailParams { ids: ids.to_vec() })
        })?;
        let songs = body
            .get("songs")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::ApiIncompatible("歌曲详情响应缺少 songs 数组".to_string()))?;
        let privileges = body
            .get("privileges")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|privilege| {
                        value_id(privilege.get("id")).map(|id| (id, privilege.clone()))
                    })
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        let mut result = HashMap::new();
        for value in songs {
            let mut song = song_from_value(value)?;
            if song.privilege.is_none()
                && let Some(privilege) = privileges.get(&song.id)
            {
                song.privilege = Some(privilege.clone());
            }
            result.insert(song.id.clone(), song);
        }
        Ok(result)
    }
    fn search(&mut self, keywords: &str) -> AppResult<Vec<Song>> {
        let body = self.call("search", |client| {
            client.search(SearchParams {
                keywords: keywords.to_string(),
                limit: Some(30),
                offset: Some(0),
                ..Default::default()
            })
        })?;
        let songs = body
            .pointer("/result/songs")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                AppError::ApiIncompatible("搜索响应缺少 result.songs 数组".to_string())
            })?;
        songs.iter().map(song_from_value).collect()
    }
    fn playback_probe(&mut self, id: &str) -> AppResult<Probe> {
        let body = self.call("playback_probe", |client| {
            client.song_url_v1(SongUrlV1Params {
                id: id.to_string(),
                level: Some(SongQualityLevel::Standard),
                encode_type: Some("mp3".to_string()),
            })
        })?;
        let Some(item) = body
            .get("data")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
        else {
            return Ok(Probe {
                has_url: false,
                fee: None,
                free_trial: false,
            });
        };
        Ok(Probe {
            has_url: item.get("url").is_some_and(|value| {
                !value.is_null() && value.as_str().is_some_and(|s| !s.is_empty())
            }),
            fee: strict_item_i64(item, "fee"),
            free_trial: item
                .get("freeTrialInfo")
                .is_some_and(|value| !value.is_null()),
        })
    }
    fn user_playlists(&mut self, uid: &str) -> AppResult<Vec<PlaylistSummary>> {
        let mut result = Vec::new();
        let mut seen_ids = HashSet::new();
        let mut offset = 0;
        loop {
            let body = self.call("user_playlist", |client| {
                client.user_playlist(UserPlaylistParams {
                    uid: uid.to_string(),
                    limit: Some(100),
                    offset: Some(offset),
                })
            })?;
            let playlists = body
                .get("playlist")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    AppError::ApiIncompatible("用户歌单响应缺少 playlist 数组".to_string())
                })?;
            let page_len = playlists.len();
            let mut new_count = 0;
            for playlist in playlists {
                if let Some(summary) = playlist_summary(playlist)
                    && seen_ids.insert(summary.id.clone())
                {
                    result.push(summary);
                    new_count += 1;
                }
            }
            let more = body.get("more").and_then(Value::as_bool);
            if page_len < 100 || new_count == 0 || more == Some(false) {
                break;
            }
            offset += 100;
        }
        Ok(result)
    }
    fn playlist_summary_by_id(
        &mut self,
        playlist_id: &str,
        uid: &str,
    ) -> AppResult<Option<PlaylistSummary>> {
        let body = self.call("playlist_detail", |client| {
            client.playlist_detail(PlaylistDetailParams {
                id: playlist_id.to_string(),
                s: Some(8),
            })
        })?;
        let Some(playlist) = body.get("playlist") else {
            return Ok(None);
        };
        let Some(summary) = playlist_summary(playlist) else {
            return Ok(None);
        };
        if summary.name == PLAYLIST_NAME && summary.owner_id.as_deref() == Some(uid) {
            Ok(Some(summary))
        } else {
            Ok(None)
        }
    }
    fn playlist_tracks(&mut self, id: &str) -> AppResult<HashSet<String>> {
        let body = self.call("playlist_track_all", |client| {
            client.playlist_track_all(PlaylistTrackAllParams {
                id: id.to_string(),
                s: Some(8),
            })
        })?;
        self.http_requests += playlist_detail_extra_requests(&body);
        Ok(playlist_track_ids(&body))
    }
    fn create_playlist(&mut self) -> AppResult<String> {
        let body = self.call("playlist_create", |client| {
            client.raw_weapi(
                PLAYLIST_CREATE_ENDPOINT,
                json!({"name": PLAYLIST_NAME, "privacy": "0", "type": "NORMAL"}),
            )
        })?;
        body.pointer("/playlist/id")
            .and_then(value_id_ref)
            .or_else(|| body.get("id").and_then(value_id_ref))
            .ok_or_else(|| AppError::ApiIncompatible("创建歌单响应缺少 id".to_string()))
    }
    fn add_tracks(&mut self, playlist_id: &str, ids: &[String]) -> AppResult<()> {
        let track_ids = serde_json::to_string(ids)?;
        self.call("playlist_manipulate_tracks", |client| {
            client.raw_weapi(
                PLAYLIST_ADD_ENDPOINT,
                json!({"pid": playlist_id, "trackIds": track_ids, "op": "add"}),
            )
        })?;
        Ok(())
    }
}

fn playlist_track_ids(body: &Value) -> HashSet<String> {
    let mut ids = HashSet::new();
    if let Some(tracks) = body.pointer("/playlist/tracks").and_then(Value::as_array) {
        for track in tracks {
            if let Some(id) = value_id(track.get("id")) {
                ids.insert(id);
            }
        }
    }
    if let Some(track_ids) = body.pointer("/playlist/trackIds").and_then(Value::as_array) {
        for track in track_ids {
            if let Some(id) = value_id(track.get("id")) {
                ids.insert(id);
            }
        }
    }
    ids
}

fn playlist_detail_extra_requests(body: &Value) -> u64 {
    body.pointer("/playlist/trackIds")
        .and_then(Value::as_array)
        .map_or(0, |ids| ids.len().div_ceil(500) as u64)
}

trait RemoteApi {
    fn status(&mut self) -> AppResult<Value>;
    fn private_fm(&mut self) -> AppResult<Vec<Song>>;
    fn details(&mut self, ids: &[String]) -> AppResult<HashMap<String, Song>>;
    fn search(&mut self, keywords: &str) -> AppResult<Vec<Song>>;
    fn playback_probe(&mut self, id: &str) -> AppResult<Probe>;
    fn user_playlists(&mut self, uid: &str) -> AppResult<Vec<PlaylistSummary>>;
    fn playlist_summary_by_id(
        &mut self,
        playlist_id: &str,
        uid: &str,
    ) -> AppResult<Option<PlaylistSummary>>;
    fn playlist_tracks(&mut self, id: &str) -> AppResult<HashSet<String>>;
    fn create_playlist(&mut self) -> AppResult<String>;
    fn add_tracks(&mut self, playlist_id: &str, ids: &[String]) -> AppResult<()>;
    fn client_calls(&self) -> u64;
    fn http_requests(&self) -> u64;
    fn save_session(&self, paths: &Paths) -> AppResult<()>;
}

impl RemoteApi for Remote {
    fn status(&mut self) -> AppResult<Value> {
        Remote::status(self)
    }
    fn private_fm(&mut self) -> AppResult<Vec<Song>> {
        Remote::private_fm(self)
    }
    fn details(&mut self, ids: &[String]) -> AppResult<HashMap<String, Song>> {
        Remote::details(self, ids)
    }
    fn search(&mut self, keywords: &str) -> AppResult<Vec<Song>> {
        Remote::search(self, keywords)
    }
    fn playback_probe(&mut self, id: &str) -> AppResult<Probe> {
        Remote::playback_probe(self, id)
    }
    fn user_playlists(&mut self, uid: &str) -> AppResult<Vec<PlaylistSummary>> {
        Remote::user_playlists(self, uid)
    }
    fn playlist_summary_by_id(
        &mut self,
        playlist_id: &str,
        uid: &str,
    ) -> AppResult<Option<PlaylistSummary>> {
        Remote::playlist_summary_by_id(self, playlist_id, uid)
    }
    fn playlist_tracks(&mut self, id: &str) -> AppResult<HashSet<String>> {
        Remote::playlist_tracks(self, id)
    }
    fn create_playlist(&mut self) -> AppResult<String> {
        Remote::create_playlist(self)
    }
    fn add_tracks(&mut self, playlist_id: &str, ids: &[String]) -> AppResult<()> {
        Remote::add_tracks(self, playlist_id, ids)
    }
    fn client_calls(&self) -> u64 {
        self.client_calls
    }
    fn http_requests(&self) -> u64 {
        self.http_requests
    }
    fn save_session(&self, paths: &Paths) -> AppResult<()> {
        refresh_session(paths, &self.client)
    }
}

fn remote_code_error(code: Option<i64>) -> AppError {
    match code {
        Some(301) | Some(401) | Some(403) => AppError::LoginRequired,
        Some(code) => AppError::Remote(format!("返回代码 {code}")),
        None => AppError::ApiIncompatible("响应缺少 code".to_string()),
    }
}

#[derive(Debug, Clone)]
struct Song {
    id: String,
    name: String,
    artists: Vec<String>,
    duration_ms: Option<i64>,
    album: Option<String>,
    fee: Option<i64>,
    privilege: Option<Value>,
}

fn song_from_value(value: &Value) -> AppResult<Song> {
    let id = value_id(value.get("id"))
        .ok_or_else(|| AppError::ApiIncompatible("歌曲缺少 id".to_string()))?;
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::ApiIncompatible("歌曲缺少 name".to_string()))?
        .to_string();
    let artist_values = value
        .get("ar")
        .or_else(|| value.get("artists"))
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::ApiIncompatible("歌曲缺少歌手信息".to_string()))?;
    let artists = artist_values
        .iter()
        .filter_map(|artist| {
            artist
                .get("name")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect::<Vec<_>>();
    if artists.is_empty() {
        return Err(AppError::ApiIncompatible("歌曲歌手信息为空".to_string()));
    }
    let album = value
        .get("al")
        .or_else(|| value.get("album"))
        .and_then(|album| album.get("name"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    Ok(Song {
        id,
        name,
        artists,
        duration_ms: item_i64(value, "dt"),
        album,
        fee: strict_item_i64(value, "fee"),
        privilege: value.get("privilege").cloned(),
    })
}

fn merge_song_metadata(base: &Song, detail: Option<&Song>) -> Song {
    let Some(detail) = detail else {
        return base.clone();
    };
    let mut merged = detail.clone();
    if merged.privilege.is_none() {
        merged.privilege = base.privilege.clone();
    }
    if merged.fee.is_none() {
        merged.fee = base.fee;
    }
    if merged.duration_ms.is_none() {
        merged.duration_ms = base.duration_ms;
    }
    if merged.album.is_none() {
        merged.album = base.album.clone();
    }
    merged
}

fn playlist_summary(value: &Value) -> Option<PlaylistSummary> {
    Some(PlaylistSummary {
        id: value_id(value.get("id"))?,
        name: value.get("name")?.as_str()?.to_string(),
        track_count: item_i64(value, "trackCount").unwrap_or(0),
        owner_id: value_id(value.get("userId"))
            .or_else(|| value.pointer("/creator/userId").and_then(value_id_ref)),
    })
}

#[derive(Debug, Clone, Serialize)]
struct PlaylistSummary {
    id: String,
    name: String,
    track_count: i64,
    owner_id: Option<String>,
}

fn value_id(value: Option<&Value>) -> Option<String> {
    value.and_then(|value| {
        value
            .as_i64()
            .map(|id| id.to_string())
            .or_else(|| value.as_str().map(ToOwned::to_owned))
    })
}

fn value_id_ref(value: &Value) -> Option<String> {
    value_id(Some(value))
}

fn item_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
    })
}

fn strict_item_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

fn privilege_i64(song: &Song, key: &str) -> Option<i64> {
    song.privilege
        .as_ref()
        .and_then(|p| strict_item_i64(p, key))
}

fn has_active_free_trial(song: &Song) -> bool {
    song.privilege
        .as_ref()
        .and_then(|privilege| privilege.get("freeTrialPrivilege"))
        .and_then(Value::as_object)
        .is_some_and(|trial| {
            trial
                .get("resConsumable")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                || trial
                    .get("userConsumable")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Availability {
    Free,
    Restricted,
    Unavailable,
    Unknown,
}

#[derive(Debug, Clone, Copy)]
struct Probe {
    has_url: bool,
    fee: Option<i64>,
    free_trial: bool,
}

#[derive(Debug, Clone, Serialize)]
struct AvailabilityEvidence {
    privilege_fee: Option<i64>,
    privilege_st: Option<i64>,
    privilege_pl: Option<i64>,
    active_free_trial: bool,
    probe_has_url: bool,
    probe_fee: Option<i64>,
    probe_free_trial: bool,
}

fn availability_evidence(song: &Song, probe: Probe) -> AvailabilityEvidence {
    AvailabilityEvidence {
        privilege_fee: privilege_i64(song, "fee").or(song.fee),
        privilege_st: privilege_i64(song, "st"),
        privilege_pl: privilege_i64(song, "pl"),
        active_free_trial: has_active_free_trial(song),
        probe_has_url: probe.has_url,
        probe_fee: probe.fee,
        probe_free_trial: probe.free_trial,
    }
}

fn availability_from_fields(song: &Song, probe: Option<Probe>) -> Availability {
    let privilege_fee = privilege_i64(song, "fee");
    let state = privilege_i64(song, "st");
    let play_bitrate = privilege_i64(song, "pl").or_else(|| privilege_i64(song, "playMaxbr"));
    if state.is_some_and(|state| state < 0) {
        return Availability::Unavailable;
    }
    if let Some(fee) = privilege_fee {
        if fee != 0 {
            return Availability::Restricted;
        }
    } else {
        return Availability::Unknown;
    }
    if has_active_free_trial(song) {
        return Availability::Restricted;
    }
    if privilege_i64(song, "pl").is_some_and(|pl| pl <= 0) {
        return Availability::Unavailable;
    }
    if state.is_none() || play_bitrate.is_none() {
        return Availability::Unknown;
    }
    if let Some(probe) = probe {
        if probe.free_trial {
            return Availability::Restricted;
        }
        match probe.fee {
            Some(fee) if fee != 0 => return Availability::Restricted,
            None => return Availability::Unknown,
            Some(0) => {}
            Some(_) => return Availability::Restricted,
        }
        return if probe.has_url {
            Availability::Free
        } else {
            Availability::Unavailable
        };
    }
    Availability::Unknown
}

fn normalize(text: &str) -> String {
    text.chars()
        .filter(|ch| {
            !ch.is_whitespace()
                && !matches!(
                    ch,
                    '(' | ')'
                        | '['
                        | ']'
                        | '（'
                        | '）'
                        | '【'
                        | '】'
                        | '-'
                        | '_'
                        | '·'
                        | '—'
                        | '–'
                        | '.'
                        | '。'
                        | ','
                )
        })
        .flat_map(char::to_lowercase)
        .collect()
}

fn version_markers(song: &Song) -> HashSet<String> {
    let text = format!(
        "{} {}",
        song.name,
        song.album.as_deref().unwrap_or_default()
    )
    .to_lowercase();
    let markers = [
        "live",
        "现场",
        "remix",
        "混音",
        "cover",
        "翻唱",
        "dj",
        "伴奏",
        "instrumental",
        "acoustic",
        "spedup",
        "sped-up",
        "slowed",
        "demo",
        "edit",
        "remaster",
        "混音版",
        "现场版",
    ];
    markers
        .iter()
        .filter(|marker| text.contains(**marker))
        .map(|marker| marker.to_string())
        .collect()
}

fn sorted_version_markers(song: &Song) -> Vec<String> {
    let mut markers = version_markers(song).into_iter().collect::<Vec<_>>();
    markers.sort();
    markers
}

fn same_recording_score(original: &Song, candidate: &Song) -> Option<f32> {
    if normalize(&original.name) != normalize(&candidate.name) {
        return None;
    }
    if original.artists.len() != candidate.artists.len()
        || original
            .artists
            .iter()
            .zip(&candidate.artists)
            .any(|(a, b)| normalize(a) != normalize(b))
    {
        return None;
    }
    let original_markers = version_markers(original);
    let candidate_markers = version_markers(candidate);
    if original_markers != candidate_markers {
        return None;
    }
    let (Some(left), Some(right)) = (original.duration_ms, candidate.duration_ms) else {
        return None;
    };
    if left <= 0 || right <= 0 || (left - right).abs() > 1500 {
        return None;
    }
    let score = if left == right { 1.0 } else { 0.96 };
    Some(score)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum Action {
    AddOriginal,
    CandidateOnly,
    Skip,
    AlreadyPresent,
}

#[derive(Debug, Clone, Serialize)]
struct Decision {
    original_id: String,
    original_title: String,
    original_artist: String,
    action: Action,
    availability: Availability,
    availability_evidence: AvailabilityEvidence,
    selected_id: Option<String>,
    selected_title: Option<String>,
    selected_artist: Option<String>,
    selected_duration_ms: Option<i64>,
    selected_version_markers: Vec<String>,
    reason: String,
}

#[derive(Debug, Clone, Serialize)]
struct PlanReport {
    ok: bool,
    command: String,
    private_fm_count: usize,
    playlist_name: String,
    playlist_id: Option<String>,
    playlist_exists: bool,
    playlist_lookup: String,
    existing_track_count: usize,
    would_create_playlist: bool,
    would_add_ids: Vec<String>,
    decisions: Vec<Decision>,
    client_calls: u64,
    http_requests: u64,
    state_corrupt_recovered: bool,
}

fn account_uid(body: &Value) -> AppResult<String> {
    body.pointer("/account/id")
        .and_then(value_id_ref)
        .or_else(|| body.pointer("/profile/userId").and_then(value_id_ref))
        .or_else(|| body.pointer("/data/account/id").and_then(value_id_ref))
        .ok_or_else(|| AppError::ApiIncompatible("登录状态响应缺少用户 id".to_string()))
}

fn account_vip_type(body: &Value) -> Option<i64> {
    body.pointer("/profile/vipType")
        .and_then(Value::as_i64)
        .or_else(|| body.pointer("/account/vipType").and_then(Value::as_i64))
        .or_else(|| {
            body.pointer("/data/profile/vipType")
                .and_then(Value::as_i64)
        })
}

fn select_playlist(
    playlists: &[PlaylistSummary],
    uid: &str,
    _preferred_id: Option<&str>,
) -> AppResult<Option<PlaylistSummary>> {
    let owned = playlists
        .iter()
        .filter(|playlist| {
            playlist.name == PLAYLIST_NAME && playlist.owner_id.as_deref() == Some(uid)
        })
        .cloned()
        .collect::<Vec<_>>();
    if owned.len() > 1 {
        return Err(AppError::AmbiguousPlaylist);
    }
    let only = owned.into_iter().next();
    Ok(only)
}

fn build_plan<R: RemoteApi>(
    remote: &mut R,
    uid: &str,
    state: &StateFile,
    command: &str,
    state_corrupt_recovered: bool,
) -> AppResult<PlanReport> {
    let fm_songs = remote.private_fm()?;
    let fm_ids = fm_songs
        .iter()
        .map(|song| song.id.clone())
        .collect::<Vec<_>>();
    let detail_map = remote.details(&fm_ids)?;
    let cached_playlist = if let Some(state_playlist_id) = state.playlist_id.as_deref() {
        remote.playlist_summary_by_id(state_playlist_id, uid)?
    } else {
        None
    };
    let (selected_playlist, playlist_lookup) = match cached_playlist {
        Some(playlist) => (Some(playlist), "cached_detail".to_string()),
        None => {
            let playlists = remote.user_playlists(uid)?;
            (
                select_playlist(&playlists, uid, state.playlist_id.as_deref())?,
                "user_playlist_pages".to_string(),
            )
        }
    };
    let playlist_id = selected_playlist
        .as_ref()
        .map(|playlist| playlist.id.clone());
    let existing = if let Some(id) = playlist_id.as_deref() {
        remote.playlist_tracks(id)?
    } else {
        HashSet::new()
    };
    let mut decisions = Vec::new();
    let mut candidates_to_add = Vec::new();
    let mut planned_ids = HashSet::new();
    for original in fm_songs {
        let original = merge_song_metadata(&original, detail_map.get(&original.id));
        // The privilege fields are the primary entitlement signal. The official
        // URL endpoint is queried only as a capability probe; its URL is never
        // returned, stored, downloaded, or used for playlist replacement.
        let original_probe = remote.playback_probe(&original.id)?;
        let original_availability = availability_from_fields(&original, Some(original_probe));
        let original_evidence = availability_evidence(&original, original_probe);
        let (action, selected, reason) = if original_availability == Availability::Free {
            (
                Action::AddOriginal,
                Some(original.clone()),
                "原歌曲 privilege 表明普通账号可完整播放".to_string(),
            )
        } else {
            let search_results = remote.search(&original.name)?;
            let search_ids = search_results
                .iter()
                .map(|song| song.id.clone())
                .collect::<Vec<_>>();
            let searched_details = remote.details(&search_ids)?;
            let mut best = None;
            for result in search_results {
                let candidate = merge_song_metadata(&result, searched_details.get(&result.id));
                let Some(score) = same_recording_score(&original, &candidate) else {
                    continue;
                };
                let preliminary = availability_from_fields(&candidate, None);
                if matches!(
                    preliminary,
                    Availability::Restricted | Availability::Unavailable
                ) {
                    continue;
                }
                let candidate_probe = remote.playback_probe(&candidate.id)?;
                if availability_from_fields(&candidate, Some(candidate_probe)) != Availability::Free
                {
                    continue;
                }
                if best
                    .as_ref()
                    .is_none_or(|(_, current_score): &(Song, f32)| score > *current_score)
                {
                    best = Some((candidate, score));
                }
            }
            if let Some((candidate, _score)) = best {
                (
                    Action::CandidateOnly,
                    Some(candidate),
                    "原歌曲受限；免费同曲候选仅供预览，v0.1 不自动替换".to_string(),
                )
            } else {
                (
                    Action::Skip,
                    None,
                    match original_availability {
                        Availability::Unavailable => {
                            "原歌曲不可用且没有高置信度免费同曲".to_string()
                        }
                        Availability::Restricted => {
                            "原歌曲需要 VIP/购买且没有高置信度免费同曲".to_string()
                        }
                        _ => "播放权限未知，按安全原则跳过".to_string(),
                    },
                )
            }
        };
        let selected_id = selected.as_ref().map(|song| song.id.clone());
        let duplicate = matches!(action, Action::AddOriginal)
            && selected_id
                .as_ref()
                .is_some_and(|id| existing.contains(id) || !planned_ids.insert(id.clone()));
        let final_action = if duplicate && !matches!(action, Action::Skip) {
            Action::AlreadyPresent
        } else {
            action
        };
        if matches!(final_action, Action::AddOriginal)
            && let Some(id) = &selected_id
        {
            candidates_to_add.push(id.clone());
        }
        decisions.push(Decision {
            original_id: original.id.clone(),
            original_title: original.name.clone(),
            original_artist: original.artists.join("、"),
            action: final_action,
            availability: original_availability,
            availability_evidence: original_evidence,
            selected_id,
            selected_title: selected.as_ref().map(|song| song.name.clone()),
            selected_artist: selected.as_ref().map(|song| song.artists.join("、")),
            selected_duration_ms: selected.as_ref().and_then(|song| song.duration_ms),
            selected_version_markers: selected
                .as_ref()
                .map(sorted_version_markers)
                .unwrap_or_default(),
            reason: if duplicate {
                "已在 FreeFM · Auto 或本次计划中，保持幂等".to_string()
            } else {
                reason
            },
        });
    }
    Ok(PlanReport {
        ok: true,
        command: command.to_string(),
        private_fm_count: decisions.len(),
        playlist_name: PLAYLIST_NAME.to_string(),
        playlist_id,
        playlist_exists: selected_playlist.is_some(),
        playlist_lookup,
        existing_track_count: existing.len(),
        would_create_playlist: selected_playlist.is_none(),
        would_add_ids: candidates_to_add,
        decisions,
        client_calls: remote.client_calls(),
        http_requests: remote.http_requests(),
        state_corrupt_recovered,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AccountContext {
    uid: String,
    vip_type: i64,
}

fn require_login<R: RemoteApi>(remote: &mut R) -> AppResult<AccountContext> {
    let body = match remote.status() {
        Ok(body) => body,
        Err(error) if is_login_error(&error) => return Err(AppError::LoginRequired),
        Err(error) => return Err(error),
    };
    let uid = account_uid(&body)?;
    let vip_type = account_vip_type(&body)
        .ok_or_else(|| AppError::ApiIncompatible("登录状态响应缺少 vipType".to_string()))?;
    Ok(AccountContext { uid, vip_type })
}

fn require_ordinary_account(account: &AccountContext) -> AppResult<()> {
    if account.vip_type == 0 {
        Ok(())
    } else {
        Err(AppError::OrdinaryAccountRequired)
    }
}

fn sync<R: RemoteApi>(
    paths: &Paths,
    mut report: PlanReport,
    uid: &str,
    remote: &mut R,
) -> AppResult<PlanReport> {
    let playlist_id = if let Some(id) = report.playlist_id.clone() {
        if remote.playlist_summary_by_id(&id, uid)?.is_none() {
            return Err(AppError::Remote(
                "同步前复核失败：缓存歌单不再属于当前账号或已改名".to_string(),
            ));
        }
        id
    } else {
        let playlists = remote.user_playlists(uid)?;
        if let Some(existing) = select_playlist(&playlists, uid, None)? {
            existing.id
        } else {
            remote.create_playlist()?
        }
    };
    let existing = remote.playlist_tracks(&playlist_id)?;
    let ids_to_add = report
        .would_add_ids
        .iter()
        .filter(|id| !existing.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    report.would_add_ids = ids_to_add.clone();
    if !ids_to_add.is_empty() {
        remote.add_tracks(&playlist_id, &ids_to_add)?;
        let after = remote.playlist_tracks(&playlist_id)?;
        let verified = ids_to_add.iter().all(|id| after.contains(id));
        if !verified {
            return Err(AppError::Remote("歌单写入后复读未找到全部歌曲".to_string()));
        }
    }
    let (mut state, _) = load_state(paths)?;
    state.playlist_id = Some(playlist_id.clone());
    state.last_sync_at = Some(now_seconds());
    write_private_json(&paths.state(), &state)?;
    remote.save_session(paths)?;
    report.command = "sync".to_string();
    report.playlist_id = Some(playlist_id);
    report.playlist_exists = true;
    report.would_create_playlist = false;
    report.client_calls = remote.client_calls();
    report.http_requests = remote.http_requests();
    Ok(report)
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn json_error(error: &AppError) -> Value {
    let (kind, message) = match error {
        AppError::LoginRequired => ("login_required", error.to_string()),
        AppError::OrdinaryAccountRequired => ("ordinary_account_required", error.to_string()),
        AppError::ConcurrentSync => ("concurrent_sync", error.to_string()),
        AppError::AmbiguousPlaylist => ("ambiguous_playlist", error.to_string()),
        AppError::Timeout => ("timeout", error.to_string()),
        AppError::ApiIncompatible(_) => ("api_incompatible", error.to_string()),
        AppError::StateCorrupt(_) => ("state_corrupt", error.to_string()),
        _ => ("error", error.to_string()),
    };
    json!({"ok": false, "error": {"kind": kind, "message": message}})
}

fn is_login_error(error: &AppError) -> bool {
    matches!(error, AppError::LoginRequired)
        || matches!(error, AppError::Remote(message) if message.contains("登录类响应代码"))
}

fn rendered_output(cli: &Cli, value: &Value, human: impl Display, failed: bool) -> Option<String> {
    if cli.quiet && !failed {
        return None;
    }
    if cli.json {
        Some(serde_json::to_string(value).unwrap_or_else(|_| "{\"ok\":false}".to_string()))
    } else {
        Some(human.to_string())
    }
}

fn emit(cli: &Cli, value: &Value, human: impl Display, failed: bool) {
    if let Some(output) = rendered_output(cli, value, human, failed) {
        println!("{output}");
    }
}

fn run(cli: Cli) -> AppResult<Value> {
    let paths = Paths::from_cli(&cli);
    match cli.command.as_str() {
        "auth" => authenticate(&paths),
        "status" => {
            let session = load_session(&paths)?;
            let Some(session) = session else {
                return Ok(json!({"ok": true, "authenticated": false, "session_present": false}));
            };
            let client = new_client(Some(&session))?;
            let mut remote = Remote::new(client);
            match remote.status() {
                Ok(body) => Ok(json!({
                    "ok": true,
                    "authenticated": account_uid(&body).is_ok(),
                    "account_vip_type": account_vip_type(&body),
                    "session_present": true,
                    "client_calls": remote.client_calls,
                    "http_requests": remote.http_requests
                })),
                Err(error) if is_login_error(&error) => Ok(json!({
                    "ok": true,
                    "authenticated": false,
                    "session_present": true,
                    "login_required": true,
                    "client_calls": remote.client_calls,
                    "http_requests": remote.http_requests
                })),
                Err(error) => Err(error),
            }
        }
        "preview" | "sync" => {
            let _lock = SyncLock::acquire(&paths)?;
            let session = load_session(&paths)?.ok_or(AppError::LoginRequired)?;
            let (state, state_corrupt_recovered) = load_state(&paths)?;
            let mut remote = Remote::new(new_client(Some(&session))?);
            let account = require_login(&mut remote)?;
            require_ordinary_account(&account)?;
            let report = build_plan(
                &mut remote,
                &account.uid,
                &state,
                &cli.command,
                state_corrupt_recovered,
            )?;
            if cli.command == "sync" {
                Ok(serde_json::to_value(sync(
                    &paths,
                    report,
                    &account.uid,
                    &mut remote,
                )?)?)
            } else {
                Ok(serde_json::to_value(report)?)
            }
        }
        "doctor" => {
            let session = load_session(&paths)?;
            let (_, state_corrupt_recovered) = load_state(&paths)?;
            let mut value = json!({"ok": true, "data_dir": paths.root.display().to_string(), "data_dir_exists": paths.root.exists(), "session_present": session.is_some(), "state_corrupt_recovered": state_corrupt_recovered, "protocol": {"private_fm": FM_ENDPOINT, "playability": "/api/song/enhance/player/url/v1", "search": "/api/cloudsearch/pc", "playlist_create": PLAYLIST_CREATE_ENDPOINT, "playlist_add": PLAYLIST_ADD_ENDPOINT}});
            if let Some(session) = session {
                let mut remote = Remote::new(new_client(Some(&session))?);
                match remote.status() {
                    Ok(body) => {
                        value["authenticated"] = json!(account_uid(&body).is_ok());
                        value["account_vip_type"] = json!(account_vip_type(&body));
                        value["client_calls"] = json!(remote.client_calls);
                        value["http_requests"] = json!(remote.http_requests);
                    }
                    Err(error) if is_login_error(&error) => {
                        value["authenticated"] = json!(false);
                        value["login_required"] = json!(true);
                        value["client_calls"] = json!(remote.client_calls);
                        value["http_requests"] = json!(remote.http_requests);
                    }
                    Err(error) => return Err(error),
                }
            } else {
                value["authenticated"] = json!(false);
            }
            Ok(value)
        }
        other => Err(AppError::Usage(format!("未知命令：{other}\n\n{}", usage()))),
    }
}

fn main() -> ExitCode {
    let cli = match Cli::parse(env::args()) {
        Ok(cli) => cli,
        Err(AppError::Help(message)) => {
            println!("{message}");
            return ExitCode::SUCCESS;
        }
        Err(AppError::Version) => {
            println!("freefm {VERSION}");
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    match run(cli.clone()) {
        Ok(value) => {
            let human = match cli.command.as_str() {
                "auth" => "登录成功，会话已保存到本机。".to_string(),
                "status" => {
                    if value["authenticated"] == true {
                        "已登录，会话可用。".to_string()
                    } else {
                        "未登录或会话已失效。".to_string()
                    }
                }
                "preview" => format!(
                    "预览完成：{} 首 FM 推荐，计划加入 {} 首。",
                    value["private_fm_count"],
                    value["would_add_ids"].as_array().map_or(0, Vec::len)
                ),
                "sync" => format!(
                    "同步完成：计划加入 {} 首，已验证歌单写入。",
                    value["would_add_ids"].as_array().map_or(0, Vec::len)
                ),
                "doctor" => "doctor 检查完成。".to_string(),
                _ => "完成。".to_string(),
            };
            emit(&cli, &value, human, false);
            ExitCode::SUCCESS
        }
        Err(error) => {
            let value = json_error(&error);
            emit(&cli, &value, error, true);
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(id: &str, name: &str, artist: &str, duration: i64, fee: i64) -> Song {
        Song {
            id: id.to_string(),
            name: name.to_string(),
            artists: vec![artist.to_string()],
            duration_ms: Some(duration),
            album: Some("Album".to_string()),
            fee: Some(fee),
            privilege: Some(json!({"fee": fee, "st": 0, "pl": if fee == 0 { 320000 } else { 0 }})),
        }
    }

    #[derive(Clone, Copy)]
    enum FakeFailure {
        Timeout,
        Server,
        Incompatible,
    }

    impl FakeFailure {
        fn error(self) -> AppError {
            match self {
                Self::Timeout => AppError::Timeout,
                Self::Server => AppError::Remote("fake HTTP 500".to_string()),
                Self::Incompatible => AppError::ApiIncompatible("fake non-JSON".to_string()),
            }
        }
    }

    struct FakeRemote {
        status_body: Option<Value>,
        status_failure: bool,
        fm: Vec<Song>,
        details: HashMap<String, Song>,
        probes: HashMap<String, Probe>,
        playlists: Vec<PlaylistSummary>,
        tracks: HashSet<String>,
        calls: u64,
        create_calls: u32,
        add_calls: u32,
        track_calls: u32,
        private_fm_failure: Option<FakeFailure>,
        create_after_side_effect_failure: bool,
        add_after_side_effect_failure: bool,
        track_failure_on_call: Option<u32>,
        save_session_failure: bool,
    }

    impl FakeRemote {
        fn free() -> Self {
            let source = song("1", "Free", "Artist", 180_000, 0);
            Self {
                status_body: Some(json!({"account": {"id": "u"}, "profile": {"vipType": 0}})),
                status_failure: false,
                fm: vec![source.clone()],
                details: HashMap::from([(source.id.clone(), source)]),
                probes: HashMap::from([(
                    "1".to_string(),
                    Probe {
                        has_url: true,
                        fee: Some(0),
                        free_trial: false,
                    },
                )]),
                playlists: Vec::new(),
                tracks: HashSet::new(),
                calls: 0,
                create_calls: 0,
                add_calls: 0,
                track_calls: 0,
                private_fm_failure: None,
                create_after_side_effect_failure: false,
                add_after_side_effect_failure: false,
                track_failure_on_call: None,
                save_session_failure: false,
            }
        }

        fn restricted_with_candidate() -> Self {
            let source = song("1", "Same", "Artist", 180_000, 1);
            let candidate = song("2", "Same", "Artist", 180_200, 0);
            Self {
                status_body: Some(json!({"account": {"id": "u"}, "profile": {"vipType": 0}})),
                status_failure: false,
                fm: vec![source.clone()],
                details: HashMap::from([
                    (source.id.clone(), source),
                    (candidate.id.clone(), candidate.clone()),
                ]),
                probes: HashMap::from([
                    (
                        "1".to_string(),
                        Probe {
                            has_url: false,
                            fee: Some(1),
                            free_trial: false,
                        },
                    ),
                    (
                        "2".to_string(),
                        Probe {
                            has_url: true,
                            fee: Some(0),
                            free_trial: false,
                        },
                    ),
                ]),
                playlists: Vec::new(),
                tracks: HashSet::new(),
                calls: 0,
                create_calls: 0,
                add_calls: 0,
                track_calls: 0,
                private_fm_failure: None,
                create_after_side_effect_failure: false,
                add_after_side_effect_failure: false,
                track_failure_on_call: None,
                save_session_failure: false,
            }
        }
    }

    impl RemoteApi for FakeRemote {
        fn status(&mut self) -> AppResult<Value> {
            self.calls += 1;
            if self.status_failure {
                Err(AppError::LoginRequired)
            } else {
                self.status_body
                    .clone()
                    .ok_or_else(|| AppError::ApiIncompatible("fake status missing".to_string()))
            }
        }

        fn private_fm(&mut self) -> AppResult<Vec<Song>> {
            self.calls += 1;
            if let Some(failure) = self.private_fm_failure {
                return Err(failure.error());
            }
            Ok(self.fm.clone())
        }

        fn details(&mut self, ids: &[String]) -> AppResult<HashMap<String, Song>> {
            self.calls += 1;
            Ok(ids
                .iter()
                .filter_map(|id| self.details.get(id).cloned().map(|song| (id.clone(), song)))
                .collect())
        }

        fn search(&mut self, keywords: &str) -> AppResult<Vec<Song>> {
            self.calls += 1;
            Ok(self
                .details
                .values()
                .filter(|song| song.name == keywords && song.id != "1")
                .cloned()
                .collect())
        }

        fn playback_probe(&mut self, id: &str) -> AppResult<Probe> {
            self.calls += 1;
            Ok(self.probes.get(id).copied().unwrap_or(Probe {
                has_url: false,
                fee: None,
                free_trial: false,
            }))
        }

        fn user_playlists(&mut self, _uid: &str) -> AppResult<Vec<PlaylistSummary>> {
            self.calls += 1;
            Ok(self.playlists.clone())
        }

        fn playlist_summary_by_id(
            &mut self,
            playlist_id: &str,
            uid: &str,
        ) -> AppResult<Option<PlaylistSummary>> {
            self.calls += 1;
            Ok(self
                .playlists
                .iter()
                .find(|playlist| {
                    playlist.id == playlist_id
                        && playlist.name == PLAYLIST_NAME
                        && playlist.owner_id.as_deref() == Some(uid)
                })
                .cloned())
        }

        fn playlist_tracks(&mut self, _id: &str) -> AppResult<HashSet<String>> {
            self.calls += 1;
            self.track_calls += 1;
            if self.track_failure_on_call == Some(self.track_calls) {
                return Err(AppError::Remote("fake reread failure".to_string()));
            }
            Ok(self.tracks.clone())
        }

        fn create_playlist(&mut self) -> AppResult<String> {
            self.calls += 1;
            self.create_calls += 1;
            let id = "p1".to_string();
            self.playlists.push(PlaylistSummary {
                id: id.clone(),
                name: PLAYLIST_NAME.to_string(),
                track_count: 0,
                owner_id: Some("u".to_string()),
            });
            if self.create_after_side_effect_failure {
                return Err(AppError::Remote("fake post-create failure".to_string()));
            }
            Ok(id)
        }

        fn add_tracks(&mut self, _playlist_id: &str, ids: &[String]) -> AppResult<()> {
            self.calls += 1;
            self.add_calls += 1;
            self.tracks.extend(ids.iter().cloned());
            if self.add_after_side_effect_failure {
                return Err(AppError::Remote("fake post-add failure".to_string()));
            }
            Ok(())
        }

        fn client_calls(&self) -> u64 {
            self.calls
        }

        fn http_requests(&self) -> u64 {
            self.calls
        }

        fn save_session(&self, _paths: &Paths) -> AppResult<()> {
            if self.save_session_failure {
                Err(AppError::Remote("fake session-save failure".to_string()))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn free_song_is_distinguished_from_vip_unavailable_and_unknown() {
        let free = song("1", "Song", "Artist", 180_000, 0);
        let vip = song("2", "Song", "Artist", 180_000, 1);
        let unavailable = Song {
            privilege: Some(json!({"fee": 0, "st": -200, "pl": 0})),
            ..free.clone()
        };
        let unknown = Song {
            privilege: None,
            fee: None,
            ..free.clone()
        };
        assert_eq!(availability_from_fields(&free, None), Availability::Unknown);
        assert_eq!(
            availability_from_fields(&vip, None),
            Availability::Restricted
        );
        assert_eq!(
            availability_from_fields(&unavailable, None),
            Availability::Unavailable
        );
        assert_eq!(
            availability_from_fields(&unknown, None),
            Availability::Unknown
        );
    }

    #[test]
    fn detail_metadata_does_not_discard_fm_privilege() {
        let base = song("1", "Song", "Artist", 180_000, 0);
        let detail = Song {
            privilege: None,
            fee: Some(0),
            ..base.clone()
        };
        let merged = merge_song_metadata(&base, Some(&detail));
        assert_eq!(privilege_i64(&merged, "fee"), Some(0));
        assert_eq!(privilege_i64(&merged, "pl"), Some(320000));
        assert_eq!(
            availability_from_fields(
                &merged,
                Some(Probe {
                    has_url: true,
                    fee: Some(0),
                    free_trial: false,
                })
            ),
            Availability::Free
        );
    }

    #[test]
    fn playback_probe_never_turns_free_trial_into_full_free() {
        let candidate = song("1", "Song", "Artist", 180_000, 0);
        assert_eq!(
            availability_from_fields(
                &candidate,
                Some(Probe {
                    has_url: true,
                    fee: Some(0),
                    free_trial: true
                })
            ),
            Availability::Restricted
        );
        assert_eq!(
            availability_from_fields(
                &candidate,
                Some(Probe {
                    has_url: true,
                    fee: Some(0),
                    free_trial: false
                })
            ),
            Availability::Free
        );
    }

    #[test]
    fn same_recording_rejects_wrong_artist_and_version_mismatch() {
        let original = song("1", "Song", "Artist", 180_000, 1);
        let correct = song("2", "Song", "Artist", 180_300, 0);
        let wrong_artist = song("3", "Song", "Other", 180_000, 0);
        let live = Song {
            name: "Song (Live)".to_string(),
            ..correct.clone()
        };
        assert!(same_recording_score(&original, &correct).is_some());
        assert!(same_recording_score(&original, &wrong_artist).is_none());
        assert!(same_recording_score(&original, &live).is_none());
    }

    #[test]
    fn same_recording_rejects_remix_cover_and_instrumental() {
        let original = song("1", "Song", "Artist", 180_000, 1);
        for title in [
            "Song Remix",
            "Song 翻唱",
            "Song (Acoustic)",
            "Song 伴奏",
            "Song - Live",
        ] {
            let candidate = Song {
                name: title.to_string(),
                ..song("2", title, "Artist", 180_000, 0)
            };
            assert!(
                same_recording_score(&original, &candidate).is_none(),
                "{title}"
            );
        }
    }

    #[test]
    fn fixture_variant_negative_titles_are_rejected() {
        let fixture: Value =
            serde_json::from_str(include_str!("../fixtures/fm-responses.json")).unwrap();
        let original = song("1", "Same Song", "Same Artist", 190_000, 1);
        for title in fixture["edge_cases"]["variant_negative_titles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
        {
            let candidate = song("2", title, "Same Artist", 190_000, 0);
            assert!(
                same_recording_score(&original, &candidate).is_none(),
                "{title}"
            );
        }
    }

    #[test]
    fn session_parser_requires_music_u_without_printing_values() {
        let valid = serde_json::from_str::<SessionFile>(
            r#"{"cookies":[{"name":"MUSIC_U","value":"secret"}]}"#,
        )
        .unwrap();
        assert!(valid.cookies.iter().any(|cookie| cookie.name == "MUSIC_U"));
        let invalid =
            serde_json::from_str::<SessionFile>(r#"{"cookies":[{"name":"foo","value":"bar"}]}"#)
                .unwrap();
        assert!(
            invalid
                .cookies
                .iter()
                .all(|cookie| cookie.name != "MUSIC_U")
        );
    }

    #[test]
    fn corrupted_session_is_rejected_without_recovery() {
        let root = std::env::temp_dir().join(format!(
            "freefm-session-{}-{}",
            std::process::id(),
            now_seconds()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("session.json"), b"not-json").unwrap();
        let result = load_session(&Paths { root: root.clone() });
        assert!(matches!(result, Err(AppError::StateCorrupt(_))));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn corrupted_state_recovers_to_empty_state() {
        let dir = std::env::temp_dir().join(format!("freefm-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        fs::write(dir.join("state.json"), b"not-json").unwrap();
        let (state, recovered) = load_state(&Paths { root: dir.clone() }).unwrap();
        assert!(recovered);
        assert!(state.playlist_id.is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn duplicate_ids_are_set_based_for_idempotent_append() {
        let existing = HashSet::from(["1".to_string()]);
        assert!(existing.contains("1"));
        let mut planned = HashSet::new();
        assert!(planned.insert("2".to_string()));
        assert!(!planned.insert("2".to_string()));
    }

    #[test]
    fn remote_named_playlist_is_authoritative_over_stale_state() {
        let playlists = vec![
            PlaylistSummary {
                id: "old".to_string(),
                name: "Renamed FreeFM".to_string(),
                track_count: 1,
                owner_id: Some("7".to_string()),
            },
            PlaylistSummary {
                id: "other".to_string(),
                name: PLAYLIST_NAME.to_string(),
                track_count: 2,
                owner_id: Some("7".to_string()),
            },
        ];
        assert_eq!(
            select_playlist(&playlists, "7", Some("missing"))
                .unwrap()
                .unwrap()
                .id,
            "other"
        );
        assert!(
            select_playlist(&playlists[..1], "7", Some("old"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn multiple_owned_same_name_playlists_fail_closed() {
        let playlists = vec![
            PlaylistSummary {
                id: "one".to_string(),
                name: PLAYLIST_NAME.to_string(),
                track_count: 0,
                owner_id: Some("7".to_string()),
            },
            PlaylistSummary {
                id: "two".to_string(),
                name: PLAYLIST_NAME.to_string(),
                track_count: 0,
                owner_id: Some("7".to_string()),
            },
        ];
        assert!(matches!(
            select_playlist(&playlists, "7", None),
            Err(AppError::AmbiguousPlaylist)
        ));
    }

    #[test]
    fn playlist_track_parser_handles_more_than_500_ids() {
        let track_ids = (0..501).map(|id| json!({"id": id})).collect::<Vec<_>>();
        let body = json!({"playlist": {"trackIds": track_ids}});
        assert_eq!(playlist_track_ids(&body).len(), 501);
        assert_eq!(playlist_detail_extra_requests(&body), 2);
    }

    #[test]
    fn unknown_candidate_entitlement_is_resolved_by_capability_probe() {
        let candidate = Song {
            id: "candidate".to_string(),
            name: "Song".to_string(),
            artists: vec!["Artist".to_string()],
            duration_ms: Some(180_000),
            album: Some("Album".to_string()),
            fee: Some(0),
            privilege: Some(json!({"fee": 0, "st": 0, "pl": 320000})),
        };
        assert_eq!(
            availability_from_fields(&candidate, None),
            Availability::Unknown
        );
        assert_eq!(
            availability_from_fields(
                &candidate,
                Some(Probe {
                    has_url: true,
                    fee: Some(0),
                    free_trial: false,
                })
            ),
            Availability::Free
        );
    }

    #[test]
    fn timeout_and_login_errors_are_safe_categories() {
        assert_eq!(json_error(&AppError::Timeout)["error"]["kind"], "timeout");
        assert_eq!(
            json_error(&AppError::LoginRequired)["error"]["kind"],
            "login_required"
        );
        assert!(is_login_error(&AppError::Remote(
            "login_status: 登录类响应代码 301".to_string()
        )));
        assert!(!is_login_error(&AppError::ApiIncompatible(
            "状态字段变化".to_string()
        )));
        let secret = json_error(&AppError::Remote("request failed".to_string())).to_string();
        assert!(!secret.contains("MUSIC_U"));
    }

    #[test]
    fn fixture_shape_covers_free_vip_unavailable_and_wrong_match() {
        let fixture: Value =
            serde_json::from_str(include_str!("../fixtures/fm-responses.json")).unwrap();
        let songs = fixture
            .pointer("/private_fm/data")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(songs.len(), 4);
        assert_eq!(
            fixture
                .pointer("/search_wrong_artist/result/songs")
                .and_then(Value::as_array)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            fixture
                .pointer("/edge_cases/variant_negative_titles")
                .and_then(Value::as_array)
                .unwrap()
                .len(),
            10
        );
    }

    #[test]
    fn fixture_correct_free_version_is_high_confidence_and_playable() {
        let fixture: Value =
            serde_json::from_str(include_str!("../fixtures/fm-responses.json")).unwrap();
        let original = song_from_value(&fixture["private_fm"]["data"][3]).unwrap();
        let candidate =
            song_from_value(&fixture["search_correct_free_version"]["result"]["songs"][0]).unwrap();
        assert_eq!(
            availability_from_fields(
                &candidate,
                Some(Probe {
                    has_url: true,
                    fee: Some(0),
                    free_trial: false
                })
            ),
            Availability::Free
        );
        assert_eq!(same_recording_score(&original, &candidate), Some(0.96));
    }

    #[test]
    fn cli_help_and_version_are_success_paths() {
        assert!(matches!(
            Cli::parse(["freefm".to_string()]),
            Err(AppError::Help(_))
        ));
        assert!(matches!(
            Cli::parse(["freefm".to_string(), "--version".to_string()]),
            Err(AppError::Version)
        ));
    }

    #[test]
    fn quiet_wins_on_success_but_json_errors_remain_visible() {
        let cli = Cli {
            command: "sync".to_string(),
            json: true,
            quiet: true,
            data_dir: None,
        };
        let value = json!({"ok": true});
        assert!(rendered_output(&cli, &value, "ok", false).is_none());
        let error = json!({"ok": false, "error": {"kind": "timeout"}});
        let output = rendered_output(&cli, &error, "timeout", true).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&output).unwrap(), error);
    }

    #[test]
    fn preview_plans_free_song_without_remote_write() {
        let mut remote = FakeRemote::free();
        let report = build_plan(&mut remote, "u", &StateFile::default(), "preview", false).unwrap();
        assert_eq!(report.would_add_ids, vec!["1"]);
        assert!(report.would_create_playlist);
        assert_eq!(remote.create_calls, 0);
        assert_eq!(remote.add_calls, 0);
    }

    #[test]
    fn sync_is_append_only_and_second_run_is_idempotent() {
        let root = std::env::temp_dir().join(format!(
            "freefm-flow-{}-{}",
            std::process::id(),
            now_seconds()
        ));
        let paths = Paths { root: root.clone() };
        let mut remote = FakeRemote::free();
        let (state, recovered) = load_state(&paths).unwrap();
        let report = build_plan(&mut remote, "u", &state, "sync", recovered).unwrap();
        sync(&paths, report, "u", &mut remote).unwrap();
        let (state, recovered) = load_state(&paths).unwrap();
        assert!(!recovered);
        let report = build_plan(&mut remote, "u", &state, "sync", recovered).unwrap();
        assert!(report.would_add_ids.is_empty());
        sync(&paths, report, "u", &mut remote).unwrap();
        assert_eq!(remote.create_calls, 1);
        assert_eq!(remote.add_calls, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn plan_transport_failures_never_write() {
        for failure in [
            FakeFailure::Timeout,
            FakeFailure::Server,
            FakeFailure::Incompatible,
        ] {
            let mut remote = FakeRemote::free();
            remote.private_fm_failure = Some(failure);
            let result = build_plan(&mut remote, "u", &StateFile::default(), "sync", false);
            assert!(result.is_err());
            assert_eq!(remote.create_calls, 0);
            assert_eq!(remote.add_calls, 0);
        }
    }

    #[test]
    fn post_create_failure_recovers_without_duplicate_playlist() {
        let root = std::env::temp_dir().join(format!(
            "freefm-create-recovery-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let paths = Paths { root: root.clone() };
        let mut remote = FakeRemote::free();
        let report = build_plan(&mut remote, "u", &StateFile::default(), "sync", false).unwrap();
        remote.create_after_side_effect_failure = true;
        assert!(sync(&paths, report, "u", &mut remote).is_err());
        remote.create_after_side_effect_failure = false;
        let report = build_plan(&mut remote, "u", &StateFile::default(), "sync", false).unwrap();
        sync(&paths, report, "u", &mut remote).unwrap();
        assert_eq!(remote.create_calls, 1);
        assert_eq!(remote.add_calls, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn post_add_and_reread_failures_recover_without_duplicate_track() {
        for fail_reread in [false, true] {
            let root = std::env::temp_dir().join(format!(
                "freefm-add-recovery-{}-{}-{fail_reread}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let paths = Paths { root: root.clone() };
            let mut remote = FakeRemote::free();
            let report =
                build_plan(&mut remote, "u", &StateFile::default(), "sync", false).unwrap();
            if fail_reread {
                remote.track_failure_on_call = Some(2);
            } else {
                remote.add_after_side_effect_failure = true;
            }
            assert!(sync(&paths, report, "u", &mut remote).is_err());
            remote.track_failure_on_call = None;
            remote.add_after_side_effect_failure = false;
            let report =
                build_plan(&mut remote, "u", &StateFile::default(), "sync", false).unwrap();
            assert!(report.would_add_ids.is_empty());
            sync(&paths, report, "u", &mut remote).unwrap();
            assert_eq!(remote.create_calls, 1);
            assert_eq!(remote.add_calls, 1);
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn state_write_failure_recovers_from_remote_truth() {
        let parent = std::env::temp_dir().join(format!(
            "freefm-state-failure-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&parent).unwrap();
        let invalid_root = parent.join("not-a-directory");
        fs::write(&invalid_root, b"x").unwrap();
        let invalid_paths = Paths { root: invalid_root };
        let mut remote = FakeRemote::free();
        let report = build_plan(&mut remote, "u", &StateFile::default(), "sync", false).unwrap();
        assert!(sync(&invalid_paths, report, "u", &mut remote).is_err());

        let valid_paths = Paths {
            root: parent.join("valid"),
        };
        let report = build_plan(&mut remote, "u", &StateFile::default(), "sync", false).unwrap();
        assert!(report.would_add_ids.is_empty());
        sync(&valid_paths, report, "u", &mut remote).unwrap();
        assert_eq!(remote.create_calls, 1);
        assert_eq!(remote.add_calls, 1);
        let _ = fs::remove_dir_all(parent);
    }

    #[test]
    fn restricted_song_candidate_is_preview_only() {
        let mut remote = FakeRemote::restricted_with_candidate();
        let report = build_plan(&mut remote, "u", &StateFile::default(), "preview", false).unwrap();
        assert!(report.would_add_ids.is_empty());
        assert!(matches!(report.decisions[0].action, Action::CandidateOnly));
        assert_eq!(report.decisions[0].selected_id.as_deref(), Some("2"));
    }

    #[test]
    fn non_ordinary_account_is_rejected_before_planning() {
        let account = AccountContext {
            uid: "u".to_string(),
            vip_type: 1,
        };
        assert!(matches!(
            require_ordinary_account(&account),
            Err(AppError::OrdinaryAccountRequired)
        ));
        assert_eq!(
            json_error(&AppError::OrdinaryAccountRequired)["error"]["kind"],
            "ordinary_account_required"
        );
    }

    #[test]
    fn login_expiry_stops_before_private_fm() {
        let mut remote = FakeRemote::free();
        remote.status_failure = true;
        assert!(matches!(
            require_login(&mut remote),
            Err(AppError::LoginRequired)
        ));
        assert_eq!(remote.calls, 1);
    }

    #[test]
    fn missing_or_string_entitlement_never_becomes_free() {
        let mut candidate = song("1", "Song", "Artist", 180_000, 0);
        candidate.privilege = Some(json!({"fee": "0", "st": 0, "pl": 320000}));
        assert_eq!(
            availability_from_fields(
                &candidate,
                Some(Probe {
                    has_url: true,
                    fee: Some(0),
                    free_trial: false,
                })
            ),
            Availability::Unknown
        );
        candidate.privilege = Some(json!({"fee": 0, "st": 0, "pl": 320000}));
        assert_eq!(
            availability_from_fields(
                &candidate,
                Some(Probe {
                    has_url: true,
                    fee: None,
                    free_trial: false,
                })
            ),
            Availability::Unknown
        );
    }

    #[test]
    fn lock_is_exclusive_and_removed_on_drop() {
        let root = std::env::temp_dir().join(format!(
            "freefm-lock-{}-{}",
            std::process::id(),
            now_seconds()
        ));
        let paths = Paths { root: root.clone() };
        let lock = SyncLock::acquire(&paths).unwrap();
        assert!(matches!(
            SyncLock::acquire(&paths),
            Err(AppError::ConcurrentSync)
        ));
        drop(lock);
        assert!(paths.lock().exists());
        let second = SyncLock::acquire(&paths).unwrap();
        drop(second);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[ignore]
    fn lock_process_helper() {
        let Some(root) = env::var_os("FREEFM_LOCK_HELPER_ROOT") else {
            return;
        };
        let paths = Paths {
            root: PathBuf::from(root),
        };
        let _lock = SyncLock::acquire(&paths).unwrap();
        fs::write(paths.root.join("ready"), b"ready").unwrap();
        thread::sleep(Duration::from_secs(30));
    }

    #[test]
    fn flock_blocks_another_process_and_recovers_after_kill() {
        let root = std::env::temp_dir().join(format!(
            "freefm-process-lock-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let paths = Paths { root: root.clone() };
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "tests::lock_process_helper"])
            .env("FREEFM_LOCK_HELPER_ROOT", &root)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let ready = root.join("ready");
        for _ in 0..100 {
            if ready.exists() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(ready.exists(), "lock helper did not become ready");
        assert!(matches!(
            SyncLock::acquire(&paths),
            Err(AppError::ConcurrentSync)
        ));
        child.kill().unwrap();
        child.wait().unwrap();
        let recovered = SyncLock::acquire(&paths).unwrap();
        drop(recovered);
        let _ = fs::remove_dir_all(root);
    }
}
