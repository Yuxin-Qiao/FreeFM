use crate::error::{AppError, AppResult};
use crate::protocol::DEFAULT_TIMEOUT;
use crate::storage::{Paths, SessionFile, StoredCookie, restrict_dir, write_private_json};
use netease_music::{LoginQrCheckParams, NeteaseMusicClient};
use qrcode::{QrCode, render::unicode::Dense1x2};
use serde_json::{Value, json};
use std::fs;
use std::io::{self, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::thread;
use std::time::{Duration, SystemTime};

pub(crate) const QR_TIMEOUT: Duration = Duration::from_secs(300);

pub(crate) fn new_client(session: Option<&SessionFile>) -> AppResult<NeteaseMusicClient> {
    let mut builder = NeteaseMusicClient::builder().timeout(DEFAULT_TIMEOUT);
    if let Some(session) = session {
        for cookie in &session.cookies {
            builder = builder.cookie(&cookie.name, &cookie.value);
        }
    }
    Ok(builder.build()?)
}

pub(crate) fn refresh_session(paths: &Paths, client: &NeteaseMusicClient) -> AppResult<()> {
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

pub(crate) fn authenticate(paths: &Paths) -> AppResult<Value> {
    let client = new_client(None)?;
    let (key_response, qr_url) = client.login_qr_key()?;
    let key = qr_key(&key_response.body, &qr_url)?;
    let qr = QrCode::new(qr_url.as_bytes())
        .map_err(|_| AppError::ApiIncompatible("二维码内容无法生成".to_string()))?;
    let rendered = qr.render::<Dense1x2>().quiet_zone(true).build();
    restrict_dir(&paths.root)?;
    let svg = qr
        .render::<qrcode::render::svg::Color>()
        .quiet_zone(true)
        .build();
    let svg_path = paths.root.join("login.svg");
    fs::write(&svg_path, svg.as_bytes())?;
    fs::set_permissions(&svg_path, fs::Permissions::from_mode(0o600))?;
    println!("请用网易云音乐官方客户端扫码并确认。");
    println!("{rendered}");
    println!("二维码图片：{}", svg_path.display());
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
