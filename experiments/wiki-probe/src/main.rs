use netease_music::{NeteaseMusicClient, SongDetailParams};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::env;
use std::fs;

#[derive(Deserialize)]
struct Cookie {
    name: String,
    value: String,
}

#[derive(Deserialize)]
struct Session {
    cookies: Vec<Cookie>,
}

fn collect_paths(value: &Value, prefix: &str, paths: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                paths.insert(path.clone());
                collect_paths(child, &path, paths);
            }
        }
        Value::Array(items) => {
            let path = format!("{prefix}[]");
            paths.insert(path.clone());
            for item in items {
                collect_paths(item, &path, paths);
            }
        }
        Value::String(text) if text.starts_with('{') || text.starts_with('[') => {
            if let Ok(parsed) = serde_json::from_str::<Value>(text) {
                collect_paths(&parsed, &format!("{prefix}<json>"), paths);
            }
        }
        _ => {}
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let session_path = args.next().ok_or("missing session path")?;
    let song_id = args.next().ok_or("missing song id")?;
    let session: Session = serde_json::from_slice(&fs::read(session_path)?)?;
    let mut builder = NeteaseMusicClient::builder();
    for cookie in session.cookies {
        builder = builder.cookie(&cookie.name, &cookie.value);
    }
    let client = builder.build()?;
    let wiki = client.raw_weapi(
        "https://music.163.com/api/song/play/about/block/page",
        json!({"songId": song_id.clone()}),
    )?;
    let detail = client.song_detail(SongDetailParams {
        ids: vec![song_id],
    })?;
    let mut paths = BTreeSet::new();
    collect_paths(&wiki.body, "wiki", &mut paths);
    collect_paths(&detail.body, "detail", &mut paths);
    for path in paths {
        println!("{path}");
    }
    Ok(())
}
