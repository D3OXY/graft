use std::{
    fs,
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
