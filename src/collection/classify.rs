use super::{Category, Rule, RuleKind};
use crate::shell::Properties;
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

fn lower_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase()
}

fn web_category(url: &str) -> Option<Category> {
    let (scheme, rest) = url.trim().split_once("://")?;
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next()?;
    let host_port = authority.rsplit('@').next()?;
    let host = host_port
        .split(':')
        .next()?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    use Category::*;
    match host.as_str() {
        "chatgpt.com" | "chat.openai.com" | "claude.ai" | "chat.deepseek.com" | "deepseek.com"
        | "www.doubao.com" | "doubao.com" | "gemini.google.com" => Some(Assistant),
        "figma.com" | "www.figma.com" | "midjourney.com" | "www.midjourney.com" => Some(Design),
        "teams.microsoft.com" | "outlook.office.com" | "outlook.live.com" => Some(Office),
        _ => None,
    }
}

fn shortcut_url(path: &Path) -> Option<String> {
    let mut buf = Vec::new();
    File::open(path)
        .ok()?
        .take(65536)
        .read_to_end(&mut buf)
        .ok()?;
    let text = if buf.starts_with(&[0xff, 0xfe]) {
        String::from_utf16_lossy(
            &buf[2..]
                .chunks_exact(2)
                .map(|p| u16::from_le_bytes([p[0], p[1]]))
                .collect::<Vec<_>>(),
        )
    } else {
        String::from_utf8_lossy(&buf)
            .trim_start_matches('\u{feff}')
            .to_owned()
    };
    let mut section = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            section = line.eq_ignore_ascii_case("[InternetShortcut]");
        } else if section
            && let Some((key, value)) = line.split_once('=')
            && key.trim().eq_ignore_ascii_case("url")
        {
            return Some(value.trim().to_owned());
        }
    }
    None
}

fn application(identity: &str) -> Option<Category> {
    use Category::*;
    let identity = identity.to_lowercase();
    if let Some((package, app)) = identity.split_once('!')
        && !app.is_empty()
    {
        match package {
            "openai.chatgpt-desktop_2p2nqsd0c76g0" => return Some(Assistant),
            "msteams_8wekyb3d8bbwe" => return Some(Office),
            _ => {}
        }
    }
    Some(match identity.as_str() {
        "wechat" | "weixin" | "wecom" | "wxwork" | "微信" | "企业微信" | "feishu" | "lark"
        | "飞书" | "teams" | "ms-teams" | "microsoft teams" | "outlook" | "winword" | "excel"
        | "powerpnt" | "wps" | "et" | "wpp" => Office,
        "chatgpt" | "claude" | "claude desktop" | "deepseek" | "doubao" | "豆包" => Assistant,
        "visual studio code"
        | "microsoft.visualstudiocode"
        | "devenv"
        | "git"
        | "git-bash"
        | "cursor"
        | "claude-code"
        | "claude code"
        | "codex"
        | "idea64"
        | "pycharm64"
        | "rustrover64" => Developer,
        "photoshop" | "adobe photoshop" | "illustrator" | "figma" | "blender" | "midjourney" => {
            Design
        }
        "chrome" | "msedge" | "firefox" | "7zfm" | "7-zip" | "powertoys" | "winrar" => Utilities,
        "vlc" | "vlc media player" | "potplayermini64" | "wmplayer" => Media,
        _ => return None,
    })
}

fn file_category(extension: &str) -> Category {
    use Category::*;
    match extension {
        "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "cpp" | "c" | "h" | "hpp" | "cs" | "go"
        | "java" | "sln" | "code-workspace" => Developer,
        "psd" | "ai" | "fig" | "sketch" | "blend" => Design,
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "md" | "csv" | "rtf"
        | "odt" | "ods" | "odp" | "epub" => Documents,
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "svg" | "webp" | "heic" | "avif" | "tif"
        | "tiff" | "ico" => Images,
        "mp3" | "wav" | "flac" | "aac" | "m4a" | "ogg" | "opus" | "mp4" | "mov" | "mkv" | "avi"
        | "webm" | "wmv" | "m4v" => Media,
        "zip" | "7z" | "rar" | "tar" | "gz" | "bz2" | "xz" | "tgz" => Archives,
        _ => Other,
    }
}

pub(super) fn classify(path: &Path, rules: &[Rule]) -> Category {
    let source_name = lower_name(path);
    let mut target: Option<PathBuf> = None;
    let mut identities = Vec::new();
    let link = source_name.ends_with(".lnk") && !path.is_dir();
    if link && let Some(props) = Properties::from_path(path) {
        let raw = props.target();
        if let Some(id) = raw
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .filter(|n| n.contains('!'))
        {
            identities.push(id.to_owned());
        }
        target = raw.filter(|p| p.exists());
        if let Some(id) = props.app_id()
            && (target.is_some() || id.contains('!'))
        {
            identities.push(id);
        }
    }
    let effective = target.as_deref().unwrap_or(path);
    let directory = effective.is_dir();
    let name = lower_name(effective);
    let ext = effective
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    if !directory && ext == "exe" {
        if let Some(props) = Properties::from_path(effective) {
            identities.extend(props.app_id());
            identities.extend(props.product());
        }
        identities.push(name.trim_end_matches(".exe").to_string());
    }
    for rule in rules {
        let pattern = rule.pattern.trim().to_lowercase();
        if pattern.is_empty() {
            continue;
        }
        let hit = match rule.kind {
            RuleKind::Name => source_name == pattern || name == pattern,
            RuleKind::Extension => {
                !directory
                    && name.ends_with(&format!(
                        ".{}",
                        pattern.trim_start_matches("*.").trim_start_matches('.')
                    ))
            }
            RuleKind::Application => identities
                .iter()
                .any(|id| id.to_lowercase() == pattern.trim_end_matches(".exe")),
        };
        if hit {
            return rule.category;
        }
    }
    if directory {
        return Category::Folders;
    }
    if let Some(group) = identities.iter().find_map(|id| application(id)) {
        return group;
    }
    if ext == "url" {
        return shortcut_url(effective)
            .as_deref()
            .and_then(web_category)
            .unwrap_or(Category::Other);
    }
    file_category(&ext)
}
