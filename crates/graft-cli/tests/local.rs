use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn temp_dir() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("graft-cli-{suffix}"));
    fs::create_dir_all(&path).expect("directory");
    path
}

fn graft(path: &PathBuf, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_graft"))
        .current_dir(path)
        .args(args)
        .output()
        .expect("graft")
}

fn graft_with_input(
    path: &PathBuf,
    credential: &PathBuf,
    args: &[&str],
    input: &[u8],
) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_graft"));
    command
        .current_dir(path)
        .env("GRAFT_CREDENTIAL_FILE", credential)
        .args(args);
    let mut child = command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("graft");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input)
        .expect("input");
    child.wait_with_output().expect("output")
}

#[test]
fn local_day_loop_has_letter_changes_and_undo() {
    let path = temp_dir();
    let init = graft(&path, &["init"]);
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    fs::write(path.join("hello.txt"), "hello\n").expect("file");
    assert!(graft(&path, &["snap", "-m", "seed"]).status.success());
    let status = graft(&path, &["status"]);
    assert!(String::from_utf8_lossy(&status.stdout).contains("Working copy clean"));
    fs::write(path.join("hello.txt"), "changed\n").expect("file");
    assert!(graft(&path, &["snap", "-m", "change"]).status.success());
    assert!(graft(&path, &["undo"]).status.success());
    assert_eq!(
        fs::read_to_string(path.join("hello.txt")).expect("file"),
        "hello\n"
    );
    let log = graft(&path, &["log"]);
    assert!(String::from_utf8_lossy(&log.stdout).contains(' '));
    let _ = fs::remove_dir_all(path);
}

#[test]
fn help_names_the_local_verbs() {
    let path = temp_dir();
    let output = graft(&path, &["--help"]);
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("snap") && help.contains("merge") && help.contains("rebuild"));
    let _ = fs::remove_dir_all(path);
}

#[test]
fn env_set_and_run_keeps_secret_out_of_the_tree() {
    let path = temp_dir();
    let credential = path.join("portable-credential");
    assert!(graft(&path, &["init"]).status.success());
    let initialized = graft_with_input(&path, &credential, &["env", "init", "dev"], b"");
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr)
    );
    let set = graft_with_input(
        &path,
        &credential,
        &["env", "set", "dev", "MARKER"],
        b"graft-marker",
    );
    assert!(
        set.status.success(),
        "{}",
        String::from_utf8_lossy(&set.stderr)
    );
    let listed = graft_with_input(&path, &credential, &["env", "ls", "dev"], b"");
    assert_eq!(String::from_utf8_lossy(&listed.stdout).trim(), "MARKER");
    let run = graft_with_input(
        &path,
        &credential,
        &[
            "env",
            "run",
            "dev",
            "--",
            "sh",
            "-c",
            "test \"$MARKER\" = graft-marker",
        ],
        b"",
    );
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(!fs::read_dir(&path).expect("tree").any(|entry| {
        let path = entry.expect("entry").path();
        path.is_file()
            && fs::read(path)
                .map(|bytes| {
                    bytes
                        .windows(b"graft-marker".len())
                        .any(|window| window == b"graft-marker")
                })
                .unwrap_or(false)
    }));
    let _ = fs::remove_dir_all(path);
}
