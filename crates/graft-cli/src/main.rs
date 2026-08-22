use std::{env, path::PathBuf, process};

use graft_core::{artifact::Revision, ChangeId, Workspace, WorkspaceError};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    if let Err(error) = run(env::args().skip(1).collect()) {
        eprintln!("graft: {error}");
        process::exit(1);
    }
}

fn run(args: Vec<String>) -> Result<(), WorkspaceError> {
    let Some(command) = args.first().map(String::as_str) else {
        print_help();
        return Ok(());
    };
    match command {
        "--version" | "-V" | "version" => println!("graft {VERSION}"),
        "--help" | "-h" => print_help(),
        "init" => init(&args[1..])?,
        "status" => status()?,
        "log" => log()?,
        "desc" => describe(&args[1..])?,
        "snap" => snap(&args[1..])?,
        "amend" => amend()?,
        "edit" => edit(&args[1..])?,
        "branch" => branch(&args[1..])?,
        "switch" => switch_branch(&args[1..])?,
        "merge" => merge(&args[1..])?,
        "undo" => Workspace::open(env::current_dir()?)?.undo()?,
        "rebuild" => Workspace::open(env::current_dir()?)?.rebuild()?,
        _ => {
            return Err(WorkspaceError::InvalidState(format!(
                "unknown command `{command}`; run `graft --help`"
            )))
        }
    }
    Ok(())
}

fn print_help() {
    println!(
        "graft {VERSION}\n\nUsage: graft <command> [options]\n\nCommands:\n  init [path]          initialize a workspace\n  status               show working-copy changes\n  log                  show revisions and Changes\n  desc -m <message>    describe the working Change\n  snap -m <message>    seal a Change and start the next one\n  amend                amend the parent Change\n  edit <change>        edit an older Change\n  branch [name]        create/switch or list branches\n  switch <name>        switch branches\n  merge <name>         merge a branch; conflicts remain data\n  undo                 undo the last mutating operation\n  rebuild              rebuild derived repository indexes\n\nChange IDs render as letters; Revision IDs render as hexadecimal."
    );
}

fn init(args: &[String]) -> Result<(), WorkspaceError> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("Usage: graft init [path]");
        return Ok(());
    }
    let path = args
        .first()
        .map(PathBuf::from)
        .unwrap_or(env::current_dir()?);
    let workspace = Workspace::init(&path)?;
    println!(
        "Initialized Graft workspace in {}",
        workspace.root().display()
    );
    println!(
        "Working Change: {}",
        display_change(&workspace.state().working_change)
    );
    Ok(())
}

fn status() -> Result<(), WorkspaceError> {
    let workspace = Workspace::open(env::current_dir()?)?;
    let status = workspace.status()?;
    println!("On branch {}", workspace.state().active_branch);
    println!(
        "Working Change {}",
        display_change(&workspace.state().working_change)
    );
    if status.conflicts {
        println!("Conflicts: unresolved");
    }
    if status.changes.is_empty() {
        println!("Working copy clean");
    } else {
        println!("Working copy changes:");
        for change in status.changes {
            let marker = match change.kind {
                graft_core::diff::ChangeKind::Added => "A",
                graft_core::diff::ChangeKind::Removed => "D",
                graft_core::diff::ChangeKind::Modified => "M",
                graft_core::diff::ChangeKind::ModeChanged => "T",
            };
            println!("  {marker} {}", change.path);
        }
    }
    Ok(())
}

fn log() -> Result<(), WorkspaceError> {
    let workspace = Workspace::open(env::current_dir()?)?;
    let current = workspace
        .state()
        .working_change
        .parse::<ChangeId>()
        .map_err(|_| WorkspaceError::InvalidState("working change".into()))?;
    let revisions = workspace.log()?;
    for id in revisions.iter().rev() {
        let Some(artifact) = workspace.store().get(*id)? else {
            continue;
        };
        let Ok(revision) = Revision::from_artifact(artifact) else {
            continue;
        };
        let marker = if revision.change_id() == current {
            "@"
        } else {
            "○"
        };
        println!(
            "{marker} {} {} {}",
            revision.change_id().to_letter_string(),
            id,
            revision.message()
        );
    }
    if revisions.is_empty() {
        println!("@ {} (working)", current.to_letter_string());
    }
    Ok(())
}

fn describe(args: &[String]) -> Result<(), WorkspaceError> {
    let message = message_arg(args)?;
    Workspace::open(env::current_dir()?)?.describe(message)?;
    Ok(())
}

fn snap(args: &[String]) -> Result<(), WorkspaceError> {
    let message = message_arg(args)?;
    let mut workspace = Workspace::open(env::current_dir()?)?;
    let id = workspace.snap(message)?;
    let revision = Revision::from_artifact(workspace.store().get(id)?.expect("just inserted"))
        .map_err(|_| WorkspaceError::InvalidState("new revision".into()))?;
    println!("Created {} {}", revision.change_id().to_letter_string(), id);
    Ok(())
}

fn amend() -> Result<(), WorkspaceError> {
    let mut workspace = Workspace::open(env::current_dir()?)?;
    let id = workspace.amend()?;
    println!("Amended {}", id);
    Ok(())
}

fn edit(args: &[String]) -> Result<(), WorkspaceError> {
    let selector = args
        .first()
        .ok_or_else(|| WorkspaceError::InvalidState("usage: graft edit <change>".into()))?;
    let mut workspace = Workspace::open(env::current_dir()?)?;
    workspace.edit(selector)?;
    println!("Editing Change {}", selector);
    Ok(())
}

fn branch(args: &[String]) -> Result<(), WorkspaceError> {
    let mut workspace = Workspace::open(env::current_dir()?)?;
    let Some(name) = args.first() else {
        for (name, target) in workspace.refs()? {
            if let Some(name) = name.strip_prefix("branches/") {
                let marker = if name == workspace.state().active_branch {
                    "*"
                } else {
                    " "
                };
                println!("{marker} {name} {target}");
            }
        }
        return Ok(());
    };
    workspace.branch(name)?;
    println!("Switched to new branch {name}");
    Ok(())
}

fn switch_branch(args: &[String]) -> Result<(), WorkspaceError> {
    let name = args
        .first()
        .ok_or_else(|| WorkspaceError::InvalidState("usage: graft switch <name>".into()))?;
    let mut workspace = Workspace::open(env::current_dir()?)?;
    workspace.switch(name)?;
    println!("Switched to {name}");
    Ok(())
}

fn merge(args: &[String]) -> Result<(), WorkspaceError> {
    let name = args
        .first()
        .ok_or_else(|| WorkspaceError::InvalidState("usage: graft merge <name>".into()))?;
    let mut workspace = Workspace::open(env::current_dir()?)?;
    if workspace.merge(name)? {
        println!("Merged {name} with conflicts; resolve the working copy and snap");
    } else {
        println!("Merged {name}");
    }
    Ok(())
}

fn message_arg(args: &[String]) -> Result<String, WorkspaceError> {
    if args.len() == 2 && args[0] == "-m" {
        return Ok(args[1].clone());
    }
    Err(WorkspaceError::InvalidState(
        "usage: graft snap|desc -m <message>".into(),
    ))
}

fn display_change(value: &str) -> String {
    value
        .parse::<ChangeId>()
        .map(ChangeId::to_letter_string)
        .unwrap_or_else(|_| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::message_arg;

    #[test]
    fn message_requires_explicit_flag() {
        assert_eq!(
            message_arg(&["-m".into(), "hello".into()]).expect("message"),
            "hello"
        );
        assert!(message_arg(&["hello".into()]).is_err());
    }
}
