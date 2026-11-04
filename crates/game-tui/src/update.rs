//! GitHub Releases 自动更新。
//!
//! 查 `latest`，下载本平台的 tar.gz，解压后用脚本换掉当前可执行文件。
//! 网络和文件名解析都在这里，界面只看结果。

use crate::util::bar;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// 仓库 `owner/name`。编译期可用 `TUI_GAME_REPO` 覆盖。
pub const DEFAULT_REPO: &str = match option_env!("TUI_GAME_REPO") {
    Some(repo) => repo,
    None => "runbrick/tui-chuanqi",
};

const USER_AGENT: &str = concat!("r-chuanqi/", env!("TUI_GAME_VERSION"));
const API_HOST: &str = "api.github.com";
const RELEASE_HOST: &str = "github.com";
const OBJECT_HOST: &str = "objects.githubusercontent.com";
const RELEASE_ASSET_HOST: &str = "release-assets.githubusercontent.com";
/// 响应头的上限，防止对端一直不结束头部。
const MAX_HEAD: usize = 64 * 1024;
/// 下载进度最多这么久报一次，避免刷爆界面。
const REPORT_EVERY: Duration = Duration::from_millis(100);

/// 查到的新版本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// 标签，例如 `v0.2.0`。
    pub tag: String,
    /// 可比较的版本号。
    pub version: String,
    /// 本平台安装包的下载地址。
    pub asset_url: String,
    /// 安装包文件名。
    pub asset_name: String,
}

/// 更新失败。给玩家看的句子，不带堆栈。
#[derive(Debug)]
pub struct UpdateError(pub String);

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UpdateError {}

/// 安装走到哪一步。后台线程发出，界面画成进度条。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// 正在连接下载服务器。
    Connecting,
    /// 正在下载。`total` 是服务器报的总字节数，没报就是 `None`。
    Downloading { done: u64, total: Option<u64> },
    /// 下载完成，正在解压。
    Extracting,
    /// 解压完成，正在复制文件、准备替换程序。
    Installing,
    /// 全部就绪。游戏退出后，脚本会换上新程序并重新启动。
    Done,
}

/// 当前程序版本。发布构建用 git 标签，不用 `Cargo.toml` 里固定的号。
pub fn current_version() -> &'static str {
    env!("TUI_GAME_VERSION")
}

/// 最新 release 是否比当前新。开发构建（debug）不检查，避免每次 `cargo run` 都弹更新。
pub fn check_latest() -> Result<Option<Release>, UpdateError> {
    if cfg!(debug_assertions) {
        return Ok(None);
    }
    check_repo(DEFAULT_REPO, current_version())
}

/// 查指定仓库，并和 `current` 比较。
pub fn check_repo(repo: &str, current: &str) -> Result<Option<Release>, UpdateError> {
    validate_repo(repo)?;
    let url = format!("https://{API_HOST}/repos/{repo}/releases/latest");
    let body = http_get(&url, Some("application/vnd.github+json"))?;
    let release = parse_latest(&body, current)?;
    Ok(release.filter(|found| version_is_newer(&found.version, current)))
}

/// 下载安装包，解压，启动替换脚本。每一步都通过 `report` 汇报进度。
/// 成功返回后调用方要尽快退出游戏，脚本等进程结束才会换文件。
pub fn install(release: &Release, report: &mut dyn FnMut(Progress)) -> Result<(), UpdateError> {
    report(Progress::Connecting);
    let exe =
        std::env::current_exe().map_err(|err| UpdateError(format!("找不到当前程序：{err}")))?;
    let dir = exe
        .parent()
        .ok_or_else(|| UpdateError("当前程序没有所在目录".to_string()))?;
    let staging = dir.join(".update");
    if staging.exists() {
        std::fs::remove_dir_all(&staging)
            .map_err(|err| UpdateError(format!("清不掉上次的更新目录：{err}")))?;
    }
    std::fs::create_dir_all(&staging)
        .map_err(|err| UpdateError(format!("建不了更新目录：{err}")))?;

    let archive_path = staging.join(&release.asset_name);
    download_file(&release.asset_url, &archive_path, &mut |done, total| {
        report(Progress::Downloading { done, total });
    })?;
    report(Progress::Extracting);
    let extracted = staging.join("files");
    std::fs::create_dir_all(&extracted)
        .map_err(|err| UpdateError(format!("建不了解压目录：{err}")))?;
    crate::install::extract_archive(&archive_path, &extracted)?;
    report(Progress::Installing);
    let new_exe = crate::install::find_binary(&extracted)?;
    if let Some(payload) = new_exe.parent() {
        crate::install::copy_tree(payload, dir, &new_exe)?;
    }
    spawn_swap(&exe, &new_exe, &staging)?;
    report(Progress::Done);
    Ok(())
}

/// 提示栏里显示的进度文字。`version` 是正在装的版本号。
pub fn progress_text(progress: Progress, version: &str) -> String {
    match progress {
        Progress::Connecting => format!("正在连接，准备下载 {version}…"),
        Progress::Downloading {
            done,
            total: Some(total),
        } if total > 0 => {
            let permille = (u128::from(done.min(total)) * 1000 / u128::from(total)) as i32;
            format!(
                "下载 {version} {} {}%  {} / {}",
                bar(permille, 1000, 20),
                permille / 10,
                megabytes(done),
                megabytes(total)
            )
        }
        Progress::Downloading { done, .. } => {
            format!("下载 {version}… 已下载 {}", megabytes(done))
        }
        Progress::Extracting => format!("下载完成，正在解压 {version}…"),
        Progress::Installing => format!("解压完成，正在安装 {version}…"),
        Progress::Done => format!("{version} 下载并安装成功，正在重启…"),
    }
}

fn megabytes(bytes: u64) -> String {
    format!("{:.1}MB", bytes as f64 / 1_048_576.0)
}

/// `v1.2.3` / `1.2.3-beta.1` 比 `current` 新才返回 true。无法解析的标签不当成更新。
pub fn version_is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(next), Some(now)) => next > now,
        _ => false,
    }
}

/// 从标签或版本字符串里取出可比较的部分。
pub fn parse_version(raw: &str) -> Option<Semver> {
    let raw = raw.trim().trim_start_matches('v').trim_start_matches('V');
    let (numbers, pre) = raw.split_once('-').unwrap_or((raw, ""));
    let mut parts = numbers.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(Semver {
        major,
        minor,
        patch,
        pre: pre.to_string(),
    })
}

/// 只比较主次修订号，以及预发布标记。正式版大于同号的预发布版。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Semver {
    /// 主版本。
    pub major: u64,
    /// 次版本。
    pub minor: u64,
    /// 修订号。
    pub patch: u64,
    /// 预发布标记，空字符串表示正式版。
    pub pre: String,
}

impl PartialOrd for Semver {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Semver {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                _ => self.pre.cmp(&other.pre),
            })
    }
}

/// 本平台 release 资源名。
pub fn asset_name_for_host() -> String {
    asset_name(std::env::consts::OS, std::env::consts::ARCH)
}

fn asset_name(os: &str, arch: &str) -> String {
    let target = match (os, arch) {
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("windows", "aarch64") => "aarch64-pc-windows-msvc",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        _ => "unknown",
    };
    format!("r-chuanqi-{target}.tar.gz")
}

fn validate_repo(repo: &str) -> Result<(), UpdateError> {
    let mut parts = repo.split('/');
    let owner = parts.next().unwrap_or("");
    let name = parts.next().unwrap_or("");
    let ok = parts.next().is_none()
        && !owner.is_empty()
        && !name.is_empty()
        && owner
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.');
    if ok {
        Ok(())
    } else {
        Err(UpdateError(format!("仓库名不合法：{repo}")))
    }
}

/// 解析 latest release JSON。测试直接喂响应体。
pub fn parse_latest(body: &str, current: &str) -> Result<Option<Release>, UpdateError> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| UpdateError("GitHub 返回的不是 JSON".to_string()))?;
    if value.get("message").is_some() && value.get("tag_name").is_none() {
        let message = value
            .get("message")
            .and_then(|item| item.as_str())
            .unwrap_or("未知错误");
        return Err(UpdateError(format!("查不到最新版本：{message}")));
    }
    let tag = value
        .get("tag_name")
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .to_string();
    let Some(version) = parse_version(&tag).map(|semver| display_version(&semver)) else {
        return Ok(None);
    };
    if !version_is_newer(&version, current) {
        return Ok(None);
    }
    let want = asset_name_for_host();
    let assets = value
        .get("assets")
        .and_then(|item| item.as_array())
        .ok_or_else(|| UpdateError("这个版本没有附件".to_string()))?;
    let asset = assets.iter().find(|asset| {
        asset
            .get("name")
            .and_then(|item| item.as_str())
            .is_some_and(|name| name == want)
    });
    let Some(asset) = asset else {
        return Err(UpdateError(format!("{tag} 没有本平台的安装包（{want}）")));
    };
    let url = asset
        .get("browser_download_url")
        .and_then(|item| item.as_str())
        .ok_or_else(|| UpdateError("安装包没有下载地址".to_string()))?;
    Ok(Some(Release {
        tag,
        version,
        asset_url: url.to_string(),
        asset_name: want,
    }))
}

fn display_version(semver: &Semver) -> String {
    if semver.pre.is_empty() {
        format!("{}.{}.{}", semver.major, semver.minor, semver.patch)
    } else {
        format!(
            "{}.{}.{}-{}",
            semver.major, semver.minor, semver.patch, semver.pre
        )
    }
}

fn http_get(url: &str, accept: Option<&str>) -> Result<String, UpdateError> {
    let mut body = Vec::new();
    download_limited(url, &mut body, 1_000_000, accept, &mut |_, _| {})?;
    String::from_utf8(body).map_err(|_| UpdateError("更新信息不是文本".to_string()))
}

fn download_file(
    url: &str,
    path: &Path,
    on_progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<(), UpdateError> {
    let mut file = File::create(path).map_err(|err| UpdateError(format!("写不了安装包：{err}")))?;
    download_limited(url, &mut file, 200_000_000, None, on_progress)?;
    Ok(())
}

fn download_limited(
    url: &str,
    output: &mut dyn Write,
    limit: u64,
    accept: Option<&str>,
    on_progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<(), UpdateError> {
    let mut current = url.to_string();
    for _ in 0..5 {
        let response = request(&current, accept)?;
        if matches!(response.status, 301 | 302 | 303 | 307 | 308) {
            current = response
                .headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("location"))
                .map(|(_, value)| resolve_redirect(&current, value))
                .ok_or_else(|| UpdateError("下载被重定向，但没有新地址".to_string()))?;
            ensure_allowed(&current)?;
            continue;
        }
        if !(200..300).contains(&response.status) {
            return Err(UpdateError(format!("下载失败（HTTP {}）", response.status)));
        }
        return copy_body(response, output, limit, on_progress);
    }
    Err(UpdateError("下载重定向次数过多".to_string()))
}

/// 把响应正文写进 `output`，边写边报进度。服务器给了 `Content-Length` 就核对收到的字节数，
/// 连接中途断掉时报错，而不是把半截文件当成功。
fn copy_body(
    response: HttpResponse,
    output: &mut dyn Write,
    limit: u64,
    on_progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<(), UpdateError> {
    let total = content_length(&response.headers);
    on_progress(0, total);
    let mut left = limit;
    let mut reader = response.body.take(limit + 1);
    let mut chunk = [0_u8; 16 * 1024];
    let mut last_report = Instant::now();
    loop {
        let read = reader
            .read(&mut chunk)
            .map_err(|err| UpdateError(format!("下载中断：{err}")))?;
        if read == 0 {
            break;
        }
        if read as u64 > left {
            return Err(UpdateError("安装包太大，已中止".to_string()));
        }
        left -= read as u64;
        output
            .write_all(&chunk[..read])
            .map_err(|err| UpdateError(format!("写不了安装包：{err}")))?;
        if last_report.elapsed() >= REPORT_EVERY {
            on_progress(limit - left, total);
            last_report = Instant::now();
        }
    }
    let done = limit - left;
    if let Some(total) = total {
        if done != total {
            return Err(UpdateError(format!(
                "下载不完整：收到 {done} 字节，应为 {total} 字节"
            )));
        }
    }
    on_progress(done, total);
    Ok(())
}

/// 正文总长。分块传输没有总长，返回 `None`。
fn content_length(headers: &[(String, String)]) -> Option<u64> {
    let find = |key: &str| {
        headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    };
    if find("transfer-encoding").is_some_and(|value| value.to_ascii_lowercase().contains("chunked"))
    {
        return None;
    }
    find("content-length")?.trim().parse().ok()
}

struct HttpResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Box<dyn Read>,
}

fn request(url: &str, accept: Option<&str>) -> Result<HttpResponse, UpdateError> {
    ensure_allowed(url)?;
    let parsed = parse_https(url)?;
    let host = parsed.host;
    let path = parsed.path;
    let stream = std::net::TcpStream::connect((host.as_str(), 443))
        .map_err(|err| UpdateError(format!("连不上 {host}：{err}")))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .map_err(|err| UpdateError(format!("设置超时失败：{err}")))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(20)))
        .map_err(|err| UpdateError(format!("设置超时失败：{err}")))?;
    let connector = native_tls::TlsConnector::new()
        .map_err(|err| UpdateError(format!("建不了安全连接：{err}")))?;
    let mut tls = connector
        .connect(&host, stream)
        .map_err(|err| UpdateError(format!("连不上 {host}：{err}")))?;
    let accept = accept.unwrap_or("*/*");
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: {USER_AGENT}\r\nAccept: {accept}\r\nConnection: close\r\n\r\n"
    );
    tls.write_all(request.as_bytes())
        .map_err(|err| UpdateError(format!("发不出请求：{err}")))?;
    read_response(tls)
}

struct HttpUrl {
    host: String,
    path: String,
}

fn parse_https(url: &str) -> Result<HttpUrl, UpdateError> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| UpdateError("只接受 https 下载地址".to_string()))?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    if host.is_empty() || host.contains('@') || host.contains(':') {
        return Err(UpdateError("下载地址不合法".to_string()));
    }
    Ok(HttpUrl {
        host: host.to_string(),
        path: format!("/{path}"),
    })
}

fn ensure_allowed(url: &str) -> Result<(), UpdateError> {
    let parsed = parse_https(url)?;
    let host = parsed.host.as_str();
    if host == API_HOST
        || host == RELEASE_HOST
        || host.ends_with(".githubusercontent.com")
        || host == OBJECT_HOST
        || host == RELEASE_ASSET_HOST
    {
        Ok(())
    } else {
        Err(UpdateError(format!("拒绝访问 {host}")))
    }
}

fn resolve_redirect(current: &str, location: &str) -> String {
    if location.starts_with("https://") {
        return location.to_string();
    }
    let Some(host) = current
        .strip_prefix("https://")
        .and_then(|rest| rest.split('/').next())
    else {
        return location.to_string();
    };
    if let Some(path) = location.strip_prefix('/') {
        format!("https://{host}/{path}")
    } else {
        format!("https://{host}/{location}")
    }
}

/// 只读到响应头结束就返回，正文留在连接里，由调用方边读边处理。
fn read_response(mut stream: impl Read + 'static) -> Result<HttpResponse, UpdateError> {
    let mut head = Vec::new();
    let mut chunk = [0_u8; 4096];
    let split = loop {
        let read = stream
            .read(&mut chunk)
            .map_err(|err| UpdateError(format!("读不到响应：{err}")))?;
        if read == 0 {
            return Err(UpdateError("响应不完整".to_string()));
        }
        head.extend_from_slice(&chunk[..read]);
        if let Some(split) = head.windows(4).position(|window| window == b"\r\n\r\n") {
            break split;
        }
        if head.len() > MAX_HEAD {
            return Err(UpdateError("响应头太长".to_string()));
        }
    };
    let (status, headers) = parse_head(&head[..split])?;
    // 读头时多读进来的正文接在连接前面。
    let leftover = std::io::Cursor::new(head[split + 4..].to_vec());
    Ok(HttpResponse {
        status,
        headers,
        body: Box::new(leftover.chain(stream)),
    })
}

fn parse_head(raw: &[u8]) -> Result<(u16, Vec<(String, String)>), UpdateError> {
    let header = std::str::from_utf8(raw).map_err(|_| UpdateError("响应头不是文本".to_string()))?;
    let mut lines = header.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| UpdateError("认不出 HTTP 状态".to_string()))?;
    let headers = lines
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.trim().to_string(), value.trim().to_string()))
        })
        .collect();
    Ok((status, headers))
}

fn spawn_swap(current: &Path, new_exe: &Path, staging: &Path) -> Result<(), UpdateError> {
    let pid = std::process::id();
    if cfg!(windows) {
        let script = staging.join("apply.cmd");
        // 等游戏退出后再换文件。换不成就不重开旧版，避免再次提示更新。
        // `n` 必须由脚本自己加。写进 format! 的 `%n%` 会被当成占位符。
        let mut body = format!(
            "@echo off\r\n\
             setlocal\r\n\
             set /a n=0\r\n\
             :wait\r\n\
             tasklist /FI \"PID eq {pid}\" /FO CSV /NH | findstr /C:\"\\\"{pid}\\\"\" >nul\r\n\
             if errorlevel 1 goto swap\r\n\
             set /a n+=1\r\n\
             if "
        );
        body.push_str("%n% GEQ 30 goto fail\r\n");
        body.push_str(&format!(
            "ping -n 2 127.0.0.1 >nul\r\n\
             goto wait\r\n\
             :swap\r\n\
             move /Y \"{}\" \"{}.old\" >nul\r\n\
             if errorlevel 1 goto fail\r\n\
             move /Y \"{}\" \"{}\" >nul\r\n\
             if errorlevel 1 goto restore\r\n\
             start \"\" \"{}\"\r\n\
             del /F /Q \"{}.old\" >nul\r\n\
             exit /b 0\r\n\
             :restore\r\n\
             move /Y \"{}.old\" \"{}\" >nul\r\n\
             :fail\r\n\
             echo 更新没有换上新程序，请关掉游戏后再按一次 U。> \"{}\\update-failed.txt\"\r\n\
             exit /b 1\r\n",
            current.display(),
            current.display(),
            new_exe.display(),
            current.display(),
            current.display(),
            current.display(),
            current.display(),
            current.display(),
            staging.display()
        ));
        std::fs::write(&script, body)
            .map_err(|err| UpdateError(format!("写不了更新脚本：{err}")))?;
        Command::new("cmd")
            .args(["/C", "start", "", "/MIN", script.to_string_lossy().as_ref()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| UpdateError(format!("启动不了更新脚本：{err}")))?;
    } else {
        let script = staging.join("apply.sh");
        let body = format!(
            "#!/bin/sh\n\
             n=0\n\
             while kill -0 {pid} 2>/dev/null; do\n\
               n=$((n + 1))\n\
               if [ \"$n\" -ge 30 ]; then\n\
                 echo '更新没有换上新程序，请关掉游戏后再按一次 U。' > \"{}\"/update-failed.txt\n\
                 exit 1\n\
               fi\n\
               sleep 1\n\
             done\n\
             mv \"{}\" \"{}.old\" || exit 1\n\
             if ! mv \"{}\" \"{}\"; then\n\
               mv \"{}.old\" \"{}\"\n\
               exit 1\n\
             fi\n\
             chmod +x \"{}\"\n\
             \"{}\" >/dev/null 2>&1 &\n\
             rm -f \"{}.old\"\n",
            staging.display(),
            current.display(),
            current.display(),
            new_exe.display(),
            current.display(),
            current.display(),
            current.display(),
            current.display(),
            current.display(),
            current.display()
        );
        std::fs::write(&script, body)
            .map_err(|err| UpdateError(format!("写不了更新脚本：{err}")))?;
        Command::new("sh")
            .arg(&script)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| UpdateError(format!("启动不了更新脚本：{err}")))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_release_compares_numerically() {
        assert!(version_is_newer("v0.2.0", "0.1.0"));
        assert!(version_is_newer("0.1.10", "0.1.9"));
        assert!(!version_is_newer("0.1.0", "0.1.0"));
        assert!(!version_is_newer("v0.1.0", "0.2.0"));
        assert!(version_is_newer("1.0.0", "1.0.0-beta.1"));
        assert!(!version_is_newer("not-a-version", "0.1.0"));
        assert!(parse_version(current_version()).is_some());
    }

    #[test]
    fn parse_latest_picks_this_platform() {
        let name = asset_name_for_host();
        let body = format!(
            r#"{{
                "tag_name": "v0.2.0",
                "assets": [
                    {{"name": "notes.txt", "browser_download_url": "https://github.com/runbrick/tui-chuanqi/releases/download/v0.2.0/notes.txt"}},
                    {{"name": "{name}", "browser_download_url": "https://github.com/runbrick/tui-chuanqi/releases/download/v0.2.0/{name}"}}
                ]
            }}"#
        );
        let release = parse_latest(&body, "0.1.0").unwrap().unwrap();
        assert_eq!(release.version, "0.2.0");
        assert_eq!(release.asset_name, name);
        assert!(release.asset_url.contains(&name));
    }

    #[test]
    fn same_version_is_not_an_update() {
        let body = r#"{"tag_name":"v0.1.0","assets":[]}"#;
        assert!(parse_latest(body, "0.1.0").unwrap().is_none());
    }

    #[test]
    fn rejects_foreign_hosts() {
        assert!(ensure_allowed("https://api.github.com/repos/a/b/releases/latest").is_ok());
        assert!(ensure_allowed("https://github.com/a/b/releases/download/v1/a.zip").is_ok());
        assert!(ensure_allowed("https://objects.githubusercontent.com/a").is_ok());
        assert!(ensure_allowed("https://example.com/game.zip").is_err());
        assert!(ensure_allowed("http://github.com/a").is_err());
    }

    /// 每次只吐几个字节的读取器，模拟网络分包。
    struct Trickle {
        data: Vec<u8>,
        pos: usize,
        step: usize,
    }

    impl Read for Trickle {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = self.step.min(buf.len()).min(self.data.len() - self.pos);
            buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
            self.pos += n;
            Ok(n)
        }
    }

    fn trickle(text: &str, step: usize) -> Trickle {
        Trickle {
            data: text.as_bytes().to_vec(),
            pos: 0,
            step,
        }
    }

    fn response(length: Option<u64>, body: Vec<u8>) -> HttpResponse {
        let headers = length
            .map(|n| vec![("Content-Length".to_string(), n.to_string())])
            .unwrap_or_default();
        HttpResponse {
            status: 200,
            headers,
            body: Box::new(std::io::Cursor::new(body)),
        }
    }

    #[test]
    fn response_head_split_across_reads_keeps_the_body() {
        let raw = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
        let mut parsed = read_response(trickle(raw, 3)).unwrap();
        assert_eq!(parsed.status, 200);
        assert_eq!(content_length(&parsed.headers), Some(5));
        let mut body = String::new();
        parsed.body.read_to_string(&mut body).unwrap();
        assert_eq!(body, "hello");
    }

    #[test]
    fn truncated_head_is_an_error() {
        assert!(read_response(trickle("HTTP/1.1 200 OK\r\nContent-Le", 4)).is_err());
    }

    #[test]
    fn chunked_response_has_no_total() {
        let headers = vec![("Transfer-Encoding".to_string(), "chunked".to_string())];
        assert_eq!(content_length(&headers), None);
    }

    #[test]
    fn copy_body_reports_start_and_finish() {
        let data = vec![7_u8; 100_000];
        let mut out = Vec::new();
        let mut seen = Vec::new();
        copy_body(
            response(Some(100_000), data),
            &mut out,
            1_000_000,
            &mut |done, total| seen.push((done, total)),
        )
        .unwrap();
        assert_eq!(out.len(), 100_000);
        assert_eq!(seen.first(), Some(&(0, Some(100_000))));
        assert_eq!(seen.last(), Some(&(100_000, Some(100_000))));
    }

    #[test]
    fn copy_body_rejects_a_short_download() {
        let mut out = Vec::new();
        let err = copy_body(
            response(Some(10), vec![1; 4]),
            &mut out,
            1_000,
            &mut |_, _| {},
        )
        .unwrap_err();
        assert!(err.0.contains("不完整"), "{err}");
    }

    #[test]
    fn copy_body_enforces_the_size_limit() {
        let mut out = Vec::new();
        let err = copy_body(response(None, vec![1; 20]), &mut out, 10, &mut |_, _| {}).unwrap_err();
        assert!(err.0.contains("太大"), "{err}");
    }

    #[test]
    fn progress_text_shows_percent_and_size() {
        let text = progress_text(
            Progress::Downloading {
                done: 5 * 1_048_576,
                total: Some(10 * 1_048_576),
            },
            "0.2.0",
        );
        assert!(text.contains("50%"), "{text}");
        assert!(text.contains("5.0MB / 10.0MB"), "{text}");
        assert!(text.contains('#'), "{text}");
    }

    #[test]
    fn progress_text_without_total_still_shows_bytes() {
        let text = progress_text(
            Progress::Downloading {
                done: 2 * 1_048_576,
                total: None,
            },
            "0.2.0",
        );
        assert!(text.contains("2.0MB"), "{text}");
        assert!(!text.contains('%'), "{text}");
    }

    #[test]
    fn progress_text_announces_success_and_restart() {
        let text = progress_text(Progress::Done, "0.2.0");
        assert!(text.contains("成功") && text.contains("重启"), "{text}");
    }
}
