use std::{
    collections::BTreeMap,
    env, fs,
    io::{self, IsTerminal, Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    time::Duration,
};

use graft_core::{
    secrets::{Credential, MemberId},
    ArtifactId, Workspace, WorkspaceError,
};
use graft_sync::{
    apply_file_frames, decode_frames, encode_frames, plan_files, Direction, Frame, Handshake,
    WireRefUpdate,
};
use serde_json::Value;

const REMOTE_FILE: &str = "remote";
const REMOTE_VIEW_FILE: &str = "remote-view";

#[derive(Clone, Debug, Eq, PartialEq)]
struct Remote {
    base: String,
    repo: String,
}

impl Remote {
    fn parse(value: &str) -> Result<Self, WorkspaceError> {
        let value = value.trim().trim_end_matches('/');
        if value.is_empty() {
            return Err(invalid("remote URL is empty"));
        }
        let (base, path) = if let Some(rest) = value.strip_prefix("http://") {
            let (authority, path) = rest.split_once('/').ok_or_else(|| {
                invalid("remote must include a repository path, for example http://hub/repo")
            })?;
            (format!("http://{authority}"), path)
        } else if let Some(rest) = value.strip_prefix("https://") {
            let _ = rest;
            return Err(invalid("HTTPS remotes are not supported by this build"));
        } else {
            let (authority, path) = value.split_once('/').ok_or_else(|| {
                invalid("remote must include a hub and repository, for example hub/repo")
            })?;
            (format!("http://{authority}"), path)
        };
        validate_base(&base)?;
        if path.is_empty() || path.contains('/') || path.contains('?') || path.contains('#') {
            return Err(invalid("repository names cannot contain path separators"));
        }
        Ok(Self {
            base,
            repo: path.to_owned(),
        })
    }

    fn from_hub_and_repo(base: String, repo: String) -> Result<Self, WorkspaceError> {
        validate_base(&base)?;
        if repo.is_empty() || repo.contains('/') || repo.contains('\0') {
            return Err(invalid("invalid repository name"));
        }
        Ok(Self { base, repo })
    }

    fn display(&self) -> String {
        format!("{}/{}", self.base, self.repo)
    }

    fn sync_path(&self) -> String {
        format!("/repo/{}/sync", self.repo)
    }
}

struct HttpResponse {
    status: u16,
    body: Vec<u8>,
}

pub fn login_command(args: &[String]) -> Result<(), WorkspaceError> {
    let hub = args
        .first()
        .ok_or_else(|| invalid("usage: graft auth login <hub-url> [--no-publish]"))?;
    if args.len() > 2 || (args.len() == 2 && args[1] != "--no-publish") {
        return Err(invalid("usage: graft auth login <hub-url> [--no-publish]"));
    }
    let base = normalize_hub(hub)?;
    println!("{base}/settings/tokens/new");
    let token = read_pat()?;
    save_token(&base, &token)?;
    set_default_hub(&base)?;
    if args.get(1).is_some_and(|arg| arg == "--no-publish") {
        println!("Stored PAT for {base}; Member Identity publication skipped");
        return Ok(());
    }
    let credential = super::load_credential()?;
    publish_member(&base, &token, &credential)?;
    println!(
        "Stored PAT for {base}; published Member Identity {} (discovery only)",
        credential.member_identity().member_id()
    );
    Ok(())
}

pub fn logout_command(args: &[String]) -> Result<(), WorkspaceError> {
    if args.len() > 1 {
        return Err(invalid("usage: graft auth logout [hub-url]"));
    }
    let base = if let Some(value) = args.first() {
        normalize_hub(value)?
    } else {
        default_hub()?.ok_or_else(|| invalid("no default Hub; pass a hub URL"))?
    };
    remove_token(&base)?;
    println!("Deleted local PAT for {base}; revoke it in the Hub UI if it should be invalidated");
    Ok(())
}

pub fn remote_command(args: &[String]) -> Result<(), WorkspaceError> {
    let root = env::current_dir()?;
    match args {
        [] => {
            let remote = read_workspace_remote(&root)?
                .ok_or_else(|| invalid("no remote configured; use graft remote set <url>"))?;
            println!("{}", remote.display());
            Ok(())
        }
        [command, value] if command == "set" => {
            let remote = Remote::parse(value)?;
            write_workspace_remote(&root, &remote)?;
            println!("Remote set to {}", remote.display());
            Ok(())
        }
        _ => Err(invalid("usage: graft remote [set <hub/repo>]")),
    }
}

pub fn clone_command(args: &[String]) -> Result<(), WorkspaceError> {
    if args.len() != 2 {
        return Err(invalid("usage: graft clone <hub/repo> <path>"));
    }
    let remote = Remote::parse(&args[0])?;
    let destination = PathBuf::from(&args[1]);
    if destination.exists() && fs::read_dir(&destination)?.next().transpose()?.is_some() {
        return Err(invalid("clone destination must be empty"));
    }
    let mut workspace = Workspace::init(&destination)?;
    write_workspace_remote(&destination, &remote)?;
    let token = load_token(&remote.base)?;
    sync_workspace(
        &mut workspace,
        &remote,
        Direction::Pull,
        false,
        token.as_deref(),
    )?;
    println!("Cloned {} into {}", remote.display(), destination.display());
    Ok(())
}

pub fn pull_command(args: &[String]) -> Result<(), WorkspaceError> {
    if !args.is_empty() {
        return Err(invalid("usage: graft pull"));
    }
    let root = env::current_dir()?;
    let remote = read_workspace_remote(&root)?
        .ok_or_else(|| invalid("no remote configured; use graft remote set <url>"))?;
    let token = load_token(&remote.base)?;
    let mut workspace = Workspace::open(&root)?;
    sync_workspace(
        &mut workspace,
        &remote,
        Direction::Pull,
        false,
        token.as_deref(),
    )?;
    println!("Pulled {}", remote.display());
    Ok(())
}

pub fn push_command(args: &[String]) -> Result<(), WorkspaceError> {
    let mut create = None;
    let mut index = 0;
    while index < args.len() {
        if args[index] != "--create" {
            return Err(invalid("usage: graft push [--create <name>]"));
        }
        index += 1;
        let name = args
            .get(index)
            .ok_or_else(|| invalid("usage: graft push [--create <name>]"))?;
        create = Some(name.clone());
        index += 1;
    }
    let root = env::current_dir()?;
    let mut workspace = Workspace::open(&root)?;
    let remote = if let Some(value) = create {
        let remote = if value.starts_with("http://") || value.starts_with("https://") {
            Remote::parse(&value)?
        } else {
            let base = default_hub()?.ok_or_else(|| {
                invalid("--create needs a default Hub; run graft auth login first")
            })?;
            Remote::from_hub_and_repo(base, value)?
        };
        let token = required_token(&remote.base)?;
        let response = json_request(
            &remote.base,
            "POST",
            "/api/repos",
            Some(&token),
            serde_json::json!({"name": remote.repo, "public": false}),
        )?;
        ensure_success(response)?;
        write_workspace_remote(&root, &remote)?;
        remote
    } else {
        read_workspace_remote(&root)?
            .ok_or_else(|| invalid("no remote configured; use graft remote set <url>"))?
    };
    let token = required_token(&remote.base)?;
    sync_workspace(&mut workspace, &remote, Direction::Push, true, Some(&token))?;
    println!("Pushed {}", remote.display());
    Ok(())
}

pub fn propose_command(args: &[String]) -> Result<(), WorkspaceError> {
    let root = env::current_dir()?;
    let workspace = Workspace::open(&root)?;
    let mut source = format!("branches/{}", workspace.state().active_branch);
    let mut target = "branches/main".to_owned();
    let mut title = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--target" => {
                index += 1;
                target = args
                    .get(index)
                    .ok_or_else(|| {
                        invalid("usage: graft propose [source] [--target ref] [--title text]")
                    })?
                    .clone();
            }
            "--title" | "-m" => {
                index += 1;
                title = Some(
                    args.get(index)
                        .ok_or_else(|| {
                            invalid("usage: graft propose [source] [--target ref] [--title text]")
                        })?
                        .clone(),
                );
            }
            value if !value.starts_with('-') && source.starts_with("branches/") => {
                source = if value.starts_with("branches/") {
                    value.to_owned()
                } else {
                    format!("branches/{value}")
                };
            }
            _ => {
                return Err(invalid(
                    "usage: graft propose [source] [--target ref] [--title text]",
                ))
            }
        }
        index += 1;
    }
    let remote = read_workspace_remote(&root)?
        .ok_or_else(|| invalid("no remote configured; use graft remote set <url>"))?;
    let token = required_token(&remote.base)?;
    let response = json_request(
        &remote.base,
        "POST",
        &format!("/api/repos/{}/proposals", remote.repo),
        Some(&token),
        serde_json::json!({"source": source, "target": target, "title": title}),
    )?;
    let response = ensure_success(response)?;
    let value: Value = serde_json::from_slice(&response.body)
        .map_err(|_| invalid("Hub returned an invalid proposal response"))?;
    let id = value
        .get("id")
        .and_then(Value::as_i64)
        .ok_or_else(|| invalid("Hub proposal response omitted its id"))?;
    println!("Opened Proposal #{id} from {source} onto {target}");
    Ok(())
}

fn sync_workspace(
    workspace: &mut Workspace,
    remote: &Remote,
    direction: Direction,
    write: bool,
    token: Option<&str>,
) -> Result<(), WorkspaceError> {
    let max = advertised_max(&remote.base)?;
    let ids = workspace.store().ids()?;
    preflight(workspace.store(), &ids, max)?;
    let mut frames = vec![
        Frame::Handshake(Handshake {
            max_artifact_bytes: max,
            ..Handshake::v1(direction)
        }),
        Frame::Igot(ids.clone()),
    ];
    if write {
        frames.extend(plan_files(workspace.store(), &ids, max).map_err(sync_failure)?);
        let remote_view = read_remote_view(workspace.root())?;
        let expected = remote_view.into_iter().collect::<BTreeMap<_, _>>();
        let updates = workspace
            .refs()?
            .into_iter()
            .map(|(name, target)| WireRefUpdate {
                expected: expected.get(&name).copied(),
                name,
                new: Some(target),
            })
            .collect();
        frames.push(Frame::RefBatch(updates));
    }
    let body = encode_frames(&frames).map_err(sync_failure)?;
    let response = request(
        remote,
        "POST",
        &remote.sync_path(),
        token,
        &body,
        "application/octet-stream",
    )?;
    let response = ensure_success(response)?;
    let response_frames = decode_frames(&response.body).map_err(sync_failure)?;
    if let Some(Frame::Error { message, .. }) = response_frames
        .iter()
        .find(|frame| matches!(frame, Frame::Error { .. }))
    {
        return Err(invalid(format!("remote sync failed: {message}")));
    }
    apply_file_frames(workspace.store(), &response_frames, max).map_err(sync_failure)?;
    let view = response_frames.iter().find_map(|frame| {
        if let Frame::View(view) = frame {
            Some(
                view.iter()
                    .map(|item| (item.name.clone(), item.target))
                    .collect::<Vec<_>>(),
            )
        } else {
            None
        }
    });
    if let Some(view) = view {
        workspace.apply_remote_view(&view)?;
        write_remote_view(workspace.root(), &view)?;
    }
    Ok(())
}

fn preflight(
    store: &graft_core::RepoStore,
    ids: &[ArtifactId],
    max: Option<u64>,
) -> Result<(), WorkspaceError> {
    for id in ids {
        let artifact = store
            .get(*id)?
            .ok_or_else(|| invalid(format!("missing local artifact {id}")))?;
        if max.is_some_and(|limit| artifact.bytes().len() as u64 > limit) {
            return Err(invalid(format!("artifact {id} exceeds the Hub size cap")));
        }
    }
    Ok(())
}

fn advertised_max(base: &str) -> Result<Option<u64>, WorkspaceError> {
    let response = request_base(base, "GET", "/health", None, &[], "")?;
    let response = ensure_success(response)?;
    let value: Value = serde_json::from_slice(&response.body)
        .map_err(|_| invalid("Hub returned an invalid health response"))?;
    Ok(value.get("max_artifact_bytes").and_then(Value::as_u64))
}

fn publish_member(base: &str, token: &str, credential: &Credential) -> Result<(), WorkspaceError> {
    let member = credential.member_identity();
    let response = json_request(
        base,
        "POST",
        "/api/auth/member",
        Some(token),
        serde_json::json!({
            "member_id": member.member_id().to_string(),
            "age_recipient": member.age_recipient(),
            "signing_public": MemberId::from_bytes(member.signing_public()).to_string()
        }),
    )?;
    ensure_success(response)?;
    Ok(())
}

fn normalize_hub(value: &str) -> Result<String, WorkspaceError> {
    let value = value.trim().trim_end_matches('/');
    let base = if value.starts_with("http://") {
        value.to_owned()
    } else if value.starts_with("https://") {
        return Err(invalid("HTTPS Hubs are not supported by this build"));
    } else {
        format!("http://{value}")
    };
    validate_base(&base)?;
    if base[7..].contains('/') {
        return Err(invalid("Hub URL must not include a repository path"));
    }
    Ok(base)
}

fn validate_base(base: &str) -> Result<(), WorkspaceError> {
    let Some(authority) = base.strip_prefix("http://") else {
        return Err(invalid("only http:// Hub URLs are supported"));
    };
    if authority.is_empty()
        || authority.contains('/')
        || authority.contains(char::is_whitespace)
        || authority.contains('?')
        || authority.contains('#')
    {
        return Err(invalid("invalid Hub URL"));
    }
    Ok(())
}

fn request(
    remote: &Remote,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: &[u8],
    content_type: &str,
) -> Result<HttpResponse, WorkspaceError> {
    request_base(&remote.base, method, path, token, body, content_type)
}

fn request_base(
    base: &str,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: &[u8],
    content_type: &str,
) -> Result<HttpResponse, WorkspaceError> {
    validate_base(base)?;
    if !path.starts_with('/') || path.contains(' ') || path.contains('\r') || path.contains('\n') {
        return Err(invalid("invalid Hub request path"));
    }
    let authority = base
        .strip_prefix("http://")
        .ok_or_else(|| invalid("only http:// Hub URLs are supported"))?;
    let address = if authority.contains(':') {
        authority.to_owned()
    } else {
        format!("{authority}:80")
    };
    let mut stream = TcpStream::connect(address)?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    if !content_type.is_empty() {
        request.push_str(&format!("Content-Type: {content_type}\r\n"));
    }
    if let Some(token) = token {
        if token.contains('\r') || token.contains('\n') {
            return Err(invalid("invalid PAT"));
        }
        request.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes)?;
    parse_response(&bytes)
}

fn json_request(
    base: &str,
    method: &str,
    path: &str,
    token: Option<&str>,
    value: Value,
) -> Result<HttpResponse, WorkspaceError> {
    let body = serde_json::to_vec(&value).map_err(|_| invalid("could not encode Hub request"))?;
    request_base(base, method, path, token, &body, "application/json")
}

fn parse_response(bytes: &[u8]) -> Result<HttpResponse, WorkspaceError> {
    let split = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| invalid("Hub returned a malformed HTTP response"))?;
    let header_bytes = &bytes[..split];
    let body = bytes[split + 4..].to_vec();
    let mut lines = header_bytes.split(|byte| *byte == b'\n');
    let status_line = lines
        .next()
        .ok_or_else(|| invalid("Hub returned an empty HTTP response"))?;
    let status_line = String::from_utf8_lossy(status_line)
        .trim_end_matches('\r')
        .to_owned();
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| invalid("Hub returned an invalid HTTP status"))?;
    let mut headers = BTreeMap::new();
    for line in lines {
        let line = String::from_utf8_lossy(line)
            .trim_end_matches('\r')
            .to_owned();
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
    }
    Ok(HttpResponse {
        status,
        body: decode_chunked(body, &headers)?,
    })
}

fn decode_chunked(
    body: Vec<u8>,
    headers: &BTreeMap<String, String>,
) -> Result<Vec<u8>, WorkspaceError> {
    if headers
        .get("transfer-encoding")
        .is_none_or(|value| !value.eq_ignore_ascii_case("chunked"))
    {
        return Ok(body);
    }
    let mut output = Vec::new();
    let mut offset = 0;
    loop {
        let line_end = body[offset..]
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or_else(|| invalid("Hub returned malformed chunked data"))?;
        let length = usize::from_str_radix(
            String::from_utf8_lossy(&body[offset..offset + line_end])
                .split(';')
                .next()
                .unwrap_or_default()
                .trim(),
            16,
        )
        .map_err(|_| invalid("Hub returned malformed chunked data"))?;
        offset += line_end + 2;
        if length == 0 {
            return Ok(output);
        }
        let end = offset
            .checked_add(length)
            .ok_or_else(|| invalid("Hub returned malformed chunked data"))?;
        if end + 2 > body.len() || &body[end..end + 2] != b"\r\n" {
            return Err(invalid("Hub returned malformed chunked data"));
        }
        output.extend_from_slice(&body[offset..end]);
        offset = end + 2;
    }
}

fn ensure_success(response: HttpResponse) -> Result<HttpResponse, WorkspaceError> {
    if (200..300).contains(&response.status) {
        return Ok(response);
    }
    let message = serde_json::from_slice::<Value>(&response.body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| format!("HTTP {}", response.status));
    Err(invalid(format!("Hub request failed: {message}")))
}

fn sync_failure(error: graft_sync::SyncError) -> WorkspaceError {
    invalid(format!("sync failed: {error}"))
}

fn read_pat() -> Result<String, WorkspaceError> {
    let mut bytes = Vec::new();
    if io::stdin().is_terminal() {
        eprint!("Paste PAT: ");
        io::stderr().flush()?;
        let mut value = String::new();
        io::stdin().read_line(&mut value)?;
        let token = value.trim().to_owned();
        if token.is_empty() {
            return Err(invalid(
                "no PAT supplied; use GRAFT_TOKEN for non-interactive commands",
            ));
        }
        return Ok(token);
    }
    io::stdin().read_to_end(&mut bytes)?;
    let token = String::from_utf8(bytes)
        .map_err(|_| invalid("PAT must be UTF-8"))?
        .trim()
        .to_owned();
    if token.is_empty() {
        return Err(invalid(
            "no PAT supplied; use GRAFT_TOKEN for non-interactive commands",
        ));
    }
    Ok(token)
}

fn required_token(base: &str) -> Result<String, WorkspaceError> {
    load_token(base)?.ok_or_else(|| {
        invalid(format!(
            "no PAT for {base}; run graft auth login or set GRAFT_TOKEN"
        ))
    })
}

fn config_dir() -> Result<PathBuf, WorkspaceError> {
    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .ok_or_else(|| invalid("cannot locate the user config directory"))?;
    Ok(PathBuf::from(home).join(".graft"))
}

fn hubs_path() -> Result<PathBuf, WorkspaceError> {
    Ok(config_dir()?.join("hubs"))
}

fn config_path() -> Result<PathBuf, WorkspaceError> {
    Ok(config_dir()?.join("config"))
}

fn load_token(base: &str) -> Result<Option<String>, WorkspaceError> {
    if let Ok(token) = env::var("GRAFT_TOKEN") {
        if !token.is_empty() {
            return Ok(Some(token));
        }
    }
    let path = hubs_path()?;
    let Ok(contents) = fs::read_to_string(path) else {
        return Ok(None);
    };
    for line in contents.lines() {
        let Some((key, token)) = line.split_once('\t') else {
            continue;
        };
        if key == base {
            return Ok(Some(token.to_owned()));
        }
    }
    Ok(None)
}

fn save_token(base: &str, token: &str) -> Result<(), WorkspaceError> {
    let path = hubs_path()?;
    let mut values = BTreeMap::new();
    if let Ok(contents) = fs::read_to_string(&path) {
        for line in contents.lines() {
            if let Some((key, value)) = line.split_once('\t') {
                values.insert(key.to_owned(), value.to_owned());
            }
        }
    }
    values.insert(base.to_owned(), token.to_owned());
    let contents = values
        .into_iter()
        .map(|(key, value)| format!("{key}\t{value}\n"))
        .collect::<String>();
    write_private(&path, contents.as_bytes())
}

fn remove_token(base: &str) -> Result<(), WorkspaceError> {
    let path = hubs_path()?;
    let Ok(contents) = fs::read_to_string(&path) else {
        return Ok(());
    };
    let contents = contents
        .lines()
        .filter(|line| line.split_once('\t').is_none_or(|(key, _)| key != base))
        .map(|line| format!("{line}\n"))
        .collect::<String>();
    write_private(&path, contents.as_bytes())
}

fn default_hub() -> Result<Option<String>, WorkspaceError> {
    let path = config_path()?;
    let Ok(contents) = fs::read_to_string(path) else {
        return Ok(None);
    };
    Ok(contents.lines().find_map(|line| {
        line.strip_prefix("default_hub=")
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    }))
}

fn set_default_hub(base: &str) -> Result<(), WorkspaceError> {
    write_private(&config_path()?, format!("default_hub={base}\n").as_bytes())
}

fn read_workspace_remote(root: &Path) -> Result<Option<Remote>, WorkspaceError> {
    let path = root.join(".graft").join(REMOTE_FILE);
    match fs::read_to_string(path) {
        Ok(value) => Ok(Some(Remote::parse(value.trim())?)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn write_workspace_remote(root: &Path, remote: &Remote) -> Result<(), WorkspaceError> {
    fs::write(root.join(".graft").join(REMOTE_FILE), remote.display())?;
    Ok(())
}

fn read_remote_view(root: &Path) -> Result<Vec<(String, ArtifactId)>, WorkspaceError> {
    let path = root.join(".graft").join(REMOTE_VIEW_FILE);
    let Ok(contents) = fs::read_to_string(path) else {
        return Ok(Vec::new());
    };
    contents
        .lines()
        .map(|line| {
            let (name, value) = line
                .split_once('\t')
                .ok_or_else(|| invalid("remote view is malformed"))?;
            let target = value
                .parse()
                .map_err(|_| invalid("remote view contains an invalid artifact id"))?;
            Ok((name.to_owned(), target))
        })
        .collect()
}

fn write_remote_view(root: &Path, view: &[(String, ArtifactId)]) -> Result<(), WorkspaceError> {
    let contents = view
        .iter()
        .map(|(name, target)| format!("{name}\t{target}\n"))
        .collect::<String>();
    fs::write(root.join(".graft").join(REMOTE_VIEW_FILE), contents)?;
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), WorkspaceError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(temporary, path)?;
    Ok(())
}

fn invalid(message: impl Into<String>) -> WorkspaceError {
    WorkspaceError::InvalidState(message.into())
}
