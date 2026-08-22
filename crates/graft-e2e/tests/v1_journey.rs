use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};

static NEXT_RUN: AtomicU64 = AtomicU64::new(0);

struct HttpResponse {
    status: u16,
    body: Vec<u8>,
}

struct HttpClient {
    address: String,
}

impl HttpClient {
    fn request(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: &[u8],
        content_type: &str,
    ) -> HttpResponse {
        assert!(path.starts_with('/'), "request path must be absolute");
        let mut stream = TcpStream::connect(&self.address)
            .unwrap_or_else(|error| panic!("connect to {}: {error}", self.address));
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .expect("read timeout");
        stream
            .set_write_timeout(Some(Duration::from_secs(30)))
            .expect("write timeout");
        let mut request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Length: {}\r\n",
            self.address,
            body.len()
        );
        if !content_type.is_empty() {
            request.push_str(&format!("Content-Type: {content_type}\r\n"));
        }
        if let Some(token) = token {
            request.push_str(&format!("Authorization: Bearer {token}\r\n"));
        }
        request.push_str("\r\n");
        stream.write_all(request.as_bytes()).expect("write request");
        stream.write_all(body).expect("write request body");
        stream.flush().expect("flush request");
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).expect("read response");
        parse_response(&bytes)
    }

    fn json(&self, method: &str, path: &str, token: Option<&str>, value: Value) -> Value {
        let body = serde_json::to_vec(&value).expect("encode JSON");
        let response = self.request(method, path, token, &body, "application/json");
        assert_success(response)
    }

    fn empty(&self, method: &str, path: &str, token: Option<&str>) -> Value {
        let response = self.request(method, path, token, &[], "application/json");
        assert_success(response)
    }
}

struct HubProcess {
    child: Child,
}

impl Drop for HubProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn v1_journey_survives_real_processes_and_restart() {
    let root = unique_temp_dir();
    fs::create_dir_all(&root).expect("create test root");
    let hub_data = root.join("hub-data");
    let restored_data = root.join("restored-hub-data");
    let alice_home = root.join("alice-home");
    let bob_home = root.join("bob-home");
    fs::create_dir_all(&alice_home).expect("alice home");
    fs::create_dir_all(&bob_home).expect("bob home");

    let graft = binary_path("graft");
    let hub_binary = binary_path("graft-hub");
    let (hub, client) = spawn_hub(&hub_binary, &hub_data);
    let hub_url = format!("http://{}", client.address);

    let alice_pat = create_user_and_token(&client, "alice", "alice password", "alice-cli");
    let bob_pat = create_user_and_token(&client, "bob", "bob password", "bob-cli");

    run_ok(
        &graft,
        &root,
        &alice_home,
        &["auth", "login", &hub_url],
        Some(format!("{alice_pat}\n").as_bytes()),
    );
    run_ok(
        &graft,
        &root,
        &bob_home,
        &["auth", "login", &hub_url],
        Some(format!("{bob_pat}\n").as_bytes()),
    );
    let bob_identity = run_ok(&graft, &root, &bob_home, &["auth", "whoami"], None);
    let bob_age_recipient = line_value(&bob_identity, "age recipient: ");
    let bob_signing_public = line_value(&bob_identity, "signing public: ");

    let alice_workspace = root.join("alice-workspace");
    run_ok(
        &graft,
        &root,
        &alice_home,
        &["init", path_string(&alice_workspace)],
        None,
    );
    fs::write(&alice_workspace.join("README.md"), "hello from Alice\n").expect("README");
    run_ok(
        &graft,
        &alice_workspace,
        &alice_home,
        &["snap", "-m", "seed"],
        None,
    );
    run_ok(
        &graft,
        &alice_workspace,
        &alice_home,
        &["env", "init", "dev"],
        None,
    );
    let marker = format!("e2e-secret-{}", NEXT_RUN.fetch_add(1, Ordering::Relaxed));
    run_ok(
        &graft,
        &alice_workspace,
        &alice_home,
        &["env", "set", "dev", "MARKER"],
        Some(marker.as_bytes()),
    );
    run_ok(
        &graft,
        &alice_workspace,
        &alice_home,
        &["push", "--create", "secret"],
        None,
    );

    client.json(
        "POST",
        "/api/repos/secret/collaborators",
        Some(&alice_pat),
        json!({"username": "bob", "mode": "rw"}),
    );
    let bob_workspace = root.join("bob-workspace");
    run_ok(
        &graft,
        &root,
        &bob_home,
        &[
            "clone",
            &format!("{hub_url}/secret"),
            path_string(&bob_workspace),
        ],
        None,
    );
    let denied = run_cli(
        &graft,
        &bob_workspace,
        &bob_home,
        &["env", "run", "dev", "--", "sh", "-c", "test -n \"$MARKER\""],
        None,
    );
    assert!(
        !denied.status.success(),
        "secret must be inaccessible before grant"
    );

    run_ok(
        &graft,
        &alice_workspace,
        &alice_home,
        &[
            "env",
            "grant",
            "dev",
            &bob_age_recipient,
            &bob_signing_public,
        ],
        None,
    );
    run_ok(&graft, &alice_workspace, &alice_home, &["push"], None);
    run_ok(&graft, &bob_workspace, &bob_home, &["pull"], None);
    let granted = run_cli(
        &graft,
        &bob_workspace,
        &bob_home,
        &["env", "run", "dev", "--", "sh", "-c", "test -n \"$MARKER\""],
        None,
    );
    assert!(
        granted.status.success(),
        "secret must be available after grant"
    );

    run_ok(&graft, &bob_workspace, &bob_home, &["branch", "dev"], None);
    append(&bob_workspace.join("README.md"), "review-one\n");
    run_ok(
        &graft,
        &bob_workspace,
        &bob_home,
        &["snap", "-m", "first review change"],
        None,
    );
    fs::write(&bob_workspace.join("notes.txt"), "review notes\n").expect("notes");
    run_ok(
        &graft,
        &bob_workspace,
        &bob_home,
        &["snap", "-m", "second review change"],
        None,
    );
    run_ok(&graft, &bob_workspace, &bob_home, &["push"], None);
    let proposal_output = run_ok(
        &graft,
        &bob_workspace,
        &bob_home,
        &["propose", "--title", "Review the dev stack"],
        None,
    );
    let proposal_id = parse_proposal_id(&proposal_output);
    let proposal_path = format!("/api/repos/secret/proposals/{proposal_id}");
    let detail = client.json("GET", &proposal_path, Some(&alice_pat), Value::Null);
    let changes = detail["changes"].as_array().expect("proposal changes");
    assert_eq!(changes.len(), 2);
    let first_change = changes[0]["change_id"].as_str().expect("change id");
    let first_revision = changes[0]["revision"].as_str().expect("revision id");
    client.json(
        "POST",
        &format!("{proposal_path}/comments"),
        Some(&alice_pat),
        json!({"body": "Please keep the stack focused."}),
    );
    client.json(
        "POST",
        &format!("{proposal_path}/threads"),
        Some(&alice_pat),
        json!({
            "revision": first_revision,
            "path": "README.md",
            "line": 1,
            "body": "Please explain this change."
        }),
    );
    client.json(
        "POST",
        &format!("{proposal_path}/approvals"),
        Some(&alice_pat),
        json!({}),
    );

    run_ok(
        &graft,
        &bob_workspace,
        &bob_home,
        &["edit", first_change],
        None,
    );
    append(&bob_workspace.join("README.md"), "review-amended\n");
    run_ok(&graft, &bob_workspace, &bob_home, &["amend"], None);
    run_ok(&graft, &bob_workspace, &bob_home, &["push"], None);
    let updated = client.json("GET", &proposal_path, Some(&alice_pat), Value::Null);
    assert_eq!(updated["threads"][0]["outdated"], Value::Bool(true));
    let interdiff = client.json(
        "GET",
        &format!("{proposal_path}/interdiff/{first_change}"),
        Some(&alice_pat),
        Value::Null,
    );
    assert!(!interdiff["changes"]
        .as_array()
        .expect("interdiff")
        .is_empty());
    client.empty("POST", &format!("{proposal_path}/land"), Some(&alice_pat));

    let fresh_workspace = root.join("fresh-workspace");
    run_ok(
        &graft,
        &root,
        &alice_home,
        &[
            "clone",
            &format!("{hub_url}/secret"),
            path_string(&fresh_workspace),
        ],
        None,
    );
    assert!(fs::read_to_string(fresh_workspace.join("README.md"))
        .expect("fresh README")
        .contains("review-amended"));
    assert_eq!(
        fs::read_to_string(fresh_workspace.join("notes.txt")).expect("fresh notes"),
        "review notes\n"
    );
    assert_secret_not_persisted(&root, marker.as_bytes());

    drop(hub);
    copy_dir(&hub_data, &restored_data);
    let (restored_hub, restored_client) = spawn_hub(&hub_binary, &restored_data);
    let restored_url = format!("http://{}", restored_client.address);
    run_ok(
        &graft,
        &root,
        &alice_home,
        &["auth", "login", &restored_url, "--no-publish"],
        Some(format!("{alice_pat}\n").as_bytes()),
    );
    let restored_workspace = root.join("restored-workspace");
    run_ok(
        &graft,
        &root,
        &alice_home,
        &[
            "clone",
            &format!("{restored_url}/secret"),
            path_string(&restored_workspace),
        ],
        None,
    );
    assert!(fs::read_to_string(restored_workspace.join("README.md"))
        .expect("restored README")
        .contains("review-amended"));
    assert_eq!(
        restored_client
            .json("GET", "/api/repos", Some(&alice_pat), Value::Null)
            .as_array()
            .expect("repo list")
            .len(),
        1
    );
    assert_secret_not_persisted(&root, marker.as_bytes());
    drop(restored_hub);
    fs::remove_dir_all(root).expect("test cleanup");
}

fn unique_temp_dir() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "graft-e2e-{}-{nonce}-{}",
        std::process::id(),
        NEXT_RUN.fetch_add(1, Ordering::Relaxed)
    ))
}

fn binary_path(name: &str) -> PathBuf {
    let variable = format!("CARGO_BIN_EXE_{}", name.replace('-', "_"));
    if let Ok(path) = std::env::var(&variable) {
        return PathBuf::from(path);
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir
        .parent()
        .expect("crate parent")
        .parent()
        .expect("workspace root");
    let path = root.join("target/debug").join(name);
    assert!(
        path.exists(),
        "{name} binary missing at {}; run cargo build -p graft-cli -p graft-hub first",
        path.display()
    );
    path
}

fn spawn_hub(binary: &Path, data_dir: &Path) -> (HubProcess, HttpClient) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("reserve hub port");
    let port = listener.local_addr().expect("hub port").port();
    drop(listener);
    let address = format!("127.0.0.1:{port}");
    let child = Command::new(binary)
        .args([
            "--data-dir",
            path_string(data_dir),
            "--bind",
            &address,
            "--open-registration",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|error| panic!("start Hub: {error}"));
    let process = HubProcess { child };
    let client = HttpClient { address };
    for _ in 0..100 {
        if TcpStream::connect(&client.address).is_ok()
            && client.request("GET", "/health", None, &[], "").status == 200
        {
            return (process, client);
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("Hub did not become healthy");
}

fn create_user_and_token(
    client: &HttpClient,
    username: &str,
    password: &str,
    name: &str,
) -> String {
    let registered = client.json(
        "POST",
        "/api/register",
        None,
        json!({"username": username, "password": password}),
    );
    assert_eq!(registered["username"], username);
    let response = client.json(
        "POST",
        "/api/tokens",
        None,
        json!({
            "username": username,
            "password": password,
            "name": name,
            "mode": "rw"
        }),
    );
    response["token"].as_str().expect("PAT").to_owned()
}

fn run_ok(binary: &Path, cwd: &Path, home: &Path, args: &[&str], stdin: Option<&[u8]>) -> String {
    let output = run_cli(binary, cwd, home, args, stdin);
    assert_command_success(args, &output);
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn run_cli(binary: &Path, cwd: &Path, home: &Path, args: &[&str], stdin: Option<&[u8]>) -> Output {
    let credential = home.join(".graft/credential");
    let mut command = Command::new(binary);
    command
        .current_dir(cwd)
        .env("HOME", home)
        .env("GRAFT_CREDENTIAL_FILE", credential)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if stdin.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    let mut child = command
        .spawn()
        .unwrap_or_else(|error| panic!("run graft {:?}: {error}", args));
    if let Some(input) = stdin {
        child
            .stdin
            .take()
            .expect("piped stdin")
            .write_all(input)
            .expect("write CLI stdin");
    }
    child.wait_with_output().expect("wait for graft")
}

fn assert_command_success(args: &[&str], output: &Output) {
    if !output.status.success() {
        panic!(
            "graft {:?} failed with {}\nstdout:\n{}\nstderr:\n{}",
            args,
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn line_value(output: &str, prefix: &str) -> String {
    output
        .lines()
        .find_map(|line| line.strip_prefix(prefix))
        .map(str::to_owned)
        .unwrap_or_else(|| panic!("missing `{prefix}` in output:\n{output}"))
}

fn parse_proposal_id(output: &str) -> i64 {
    let marker = "Proposal #";
    let start = output.find(marker).expect("proposal id");
    output[start + marker.len()..]
        .split_whitespace()
        .next()
        .expect("proposal number")
        .parse()
        .expect("numeric proposal id")
}

fn path_string(path: &Path) -> &str {
    path.to_str().expect("UTF-8 test path")
}

fn append(path: &Path, value: &str) {
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap_or_else(|error| panic!("open {}: {error}", path.display()));
    file.write_all(value.as_bytes()).expect("append file");
}

fn copy_dir(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("create copy destination");
    for entry in fs::read_dir(source).expect("read source directory") {
        let entry = entry.expect("directory entry");
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir(&source_path, &destination_path);
        } else {
            fs::copy(&source_path, &destination_path)
                .unwrap_or_else(|error| panic!("copy {}: {error}", source_path.display()));
        }
    }
}

fn assert_secret_not_persisted(root: &Path, secret: &[u8]) {
    let mut files = vec![root.to_owned()];
    while let Some(path) = files.pop() {
        let metadata = fs::symlink_metadata(&path).expect("inspect test path");
        if metadata.is_dir() {
            files.extend(
                fs::read_dir(&path)
                    .expect("read test directory")
                    .map(|entry| entry.expect("directory entry").path()),
            );
        } else if metadata.is_file() {
            let bytes = fs::read(&path).expect("read test file");
            assert!(
                !bytes.windows(secret.len()).any(|window| window == secret),
                "secret leaked into {}",
                path.display()
            );
        }
    }
}

fn parse_response(bytes: &[u8]) -> HttpResponse {
    let split = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("HTTP response headers");
    let mut lines = bytes[..split].split(|byte| *byte == b'\n');
    let status_line = String::from_utf8_lossy(lines.next().expect("HTTP status line"))
        .trim_end_matches('\r')
        .to_owned();
    let status = status_line
        .split_whitespace()
        .nth(1)
        .expect("HTTP status")
        .parse()
        .expect("numeric HTTP status");
    let mut headers = BTreeMap::new();
    for line in lines {
        let line = String::from_utf8_lossy(line)
            .trim_end_matches('\r')
            .to_owned();
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    let body = decode_chunked(bytes[split + 4..].to_vec(), &headers);
    HttpResponse { status, body }
}

fn decode_chunked(body: Vec<u8>, headers: &BTreeMap<String, String>) -> Vec<u8> {
    if headers
        .get("transfer-encoding")
        .is_none_or(|value| !value.eq_ignore_ascii_case("chunked"))
    {
        return body;
    }
    let mut output = Vec::new();
    let mut offset = 0;
    loop {
        let end = body[offset..]
            .windows(2)
            .position(|window| window == b"\r\n")
            .expect("chunk length");
        let length = usize::from_str_radix(
            String::from_utf8_lossy(&body[offset..offset + end])
                .split(';')
                .next()
                .expect("chunk length")
                .trim(),
            16,
        )
        .expect("hex chunk length");
        offset += end + 2;
        if length == 0 {
            return output;
        }
        let chunk_end = offset + length;
        output.extend_from_slice(&body[offset..chunk_end]);
        assert_eq!(&body[chunk_end..chunk_end + 2], b"\r\n");
        offset = chunk_end + 2;
    }
}

fn assert_success(response: HttpResponse) -> Value {
    assert!(
        (200..300).contains(&response.status),
        "HTTP {}: {}",
        response.status,
        String::from_utf8_lossy(&response.body)
    );
    if response.body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&response.body).expect("JSON response")
    }
}
