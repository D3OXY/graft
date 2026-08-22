use std::{
    env, fs,
    io::{self, IsTerminal, Read, Write},
    path::PathBuf,
    process,
};

use graft_core::{
    artifact::Revision,
    git_import::import_git,
    secrets::{
        create_empty_environment, parse_dotenv, validate_secret_name, Credential,
        EnvironmentGeneration, Keyring, MemberId, MemberIdentity, SecretBundle,
    },
    ChangeId, Workspace, WorkspaceError,
};

mod remote;

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
        "import" => git_import_command(&args[1..])?,
        "clone" => remote::clone_command(&args[1..])?,
        "pull" => remote::pull_command(&args[1..])?,
        "push" => remote::push_command(&args[1..])?,
        "propose" => remote::propose_command(&args[1..])?,
        "remote" => remote::remote_command(&args[1..])?,
        "env" => secrets_env(&args[1..])?,
        "auth" => auth(&args[1..])?,
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
        "graft {VERSION}\n\nUsage: graft <command> [options]\n\nCommands:\n  init [path]          initialize a workspace\n  status               show working-copy changes\n  log                  show revisions and Changes\n  desc -m <message>    describe the working Change\n  snap -m <message>    seal a Change and start the next one\n  amend                amend the parent Change\n  edit <change>        edit an older Change\n  branch [name]        create/switch or list branches\n  switch <name>        switch branches\n  merge <name>         merge a branch; conflicts remain data\n  clone <hub/repo> <path>\n                       clone into a new workspace\n  pull                 fetch the configured remote\n  push [--create name] publish local artifacts and refs\n  remote [set <url>]  show or set the workspace remote\n  undo                 undo the last mutating operation\n  rebuild              rebuild derived repository indexes\n\nChange IDs render as letters; Revision IDs render as hexadecimal."
    );
    println!(
        "  env init|ls|set|unset|grant|revoke|rotate|run\n  auth whoami|login|logout|export|import"
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

fn git_import_command(args: &[String]) -> Result<(), WorkspaceError> {
    if args.len() != 2 {
        return Err(WorkspaceError::InvalidState(
            "usage: graft import <git-source> <fresh-destination>".into(),
        ));
    }
    let result = import_git(&args[0], &args[1])
        .map_err(|error| WorkspaceError::InvalidState(error.to_string()))?;
    for warning in result.warnings {
        eprintln!("warning: {warning}");
    }
    println!("Imported {} branch(es)", result.heads.len());
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

fn secrets_env(args: &[String]) -> Result<(), WorkspaceError> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(WorkspaceError::InvalidState(
            "usage: graft env init|ls|set|unset|grant|revoke|rotate|run".into(),
        ));
    };
    match command {
        "init" => env_init(&args[1..]),
        "ls" => env_ls(&args[1..]),
        "set" => env_set(&args[1..]),
        "unset" => env_unset(&args[1..]),
        "grant" => env_grant(&args[1..]),
        "revoke" => env_revoke(&args[1..]),
        "rotate" => env_rotate(&args[1..]),
        "run" => env_run(&args[1..]),
        "--help" | "-h" => {
            println!("Usage: graft env init <name> | ls [name] | set <env> <NAME> | unset <env> <NAME> | grant <env> <age-recipient> <ed25519-public-hex> | revoke <env> <member-id> | rotate <env> | run <env> -- <command>");
            Ok(())
        }
        _ => Err(WorkspaceError::InvalidState(format!(
            "unknown env command `{command}`"
        ))),
    }
}

fn auth(args: &[String]) -> Result<(), WorkspaceError> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(WorkspaceError::InvalidState(
            "usage: graft auth whoami|login|logout|export|import".into(),
        ));
    };
    match command {
        "whoami" => {
            let credential = load_credential()?;
            let member = credential.member_identity();
            println!("age recipient: {}", member.age_recipient());
            println!("member fingerprint: {}", member.member_id());
            Ok(())
        }
        "login" => remote::login_command(&args[1..]),
        "logout" => remote::logout_command(&args[1..]),
        "export" => auth_export(&args[1..]),
        "import" => auth_import(&args[1..]),
        "--help" | "-h" => {
            println!("Usage: graft auth whoami | login <hub-url> [--no-publish] | logout [hub-url] | export --yes [path] | import <path>");
            Ok(())
        }
        _ => Err(WorkspaceError::InvalidState(format!(
            "unknown auth command `{command}`"
        ))),
    }
}

fn env_init(args: &[String]) -> Result<(), WorkspaceError> {
    let name = args
        .first()
        .ok_or_else(|| WorkspaceError::InvalidState("usage: graft env init <name>".into()))?;
    let workspace = Workspace::open(env::current_dir()?)?;
    let credential = load_credential()?;
    let domain = ensure_secret_domain(&workspace)?;
    let (_, bundle) = create_empty_environment(workspace.store(), domain, name, &credential)?;
    println!("Initialized environment {name} with bundle {bundle}");
    Ok(())
}

fn env_ls(args: &[String]) -> Result<(), WorkspaceError> {
    let workspace = Workspace::open(env::current_dir()?)?;
    let credential = load_credential()?;
    let Some(keyring_id) = workspace.store().get_ref("secrets/keyring")? else {
        println!("No environments");
        return Ok(());
    };
    let keyring = load_keyring(&workspace, keyring_id)?;
    if let Some(name) = args.first() {
        let identity = keyring.unlock_environment(name, &credential)?;
        let bundle_id = workspace
            .store()
            .get_ref(&format!("secrets/{name}"))?
            .ok_or_else(|| WorkspaceError::InvalidState("environment bundle is missing".into()))?;
        let bundle = load_bundle(&workspace, bundle_id)?;
        let secrets = bundle.decrypt(&identity)?;
        for name in secrets.names() {
            println!("{name}");
        }
    } else {
        let member = credential.member_identity().member_id();
        for (name, generation) in &keyring.environments {
            let membership = if keyring.members.contains_key(&member) {
                "member"
            } else {
                "not a member"
            };
            println!("{name} generation {} ({membership})", generation.generation);
        }
    }
    Ok(())
}

fn env_set(args: &[String]) -> Result<(), WorkspaceError> {
    let env_name = args
        .first()
        .ok_or_else(|| WorkspaceError::InvalidState("usage: graft env set <env> <NAME>".into()))?;
    if args.len() == 3 && args[1] == "--from-dotenv" {
        let file = fs::read(&args[2])?;
        let parsed = parse_dotenv(&file)?;
        let values = parsed
            .names()
            .map(|name| {
                (
                    name.to_owned(),
                    parsed.get(name).expect("name exists").to_vec(),
                )
            })
            .collect::<Vec<_>>();
        update_bundle_values(env_name, values)
    } else if args.len() == 2 {
        let name = args[1].clone();
        validate_secret_name(&name)?;
        update_bundle_values(env_name, vec![(name, read_secret_value()?)])
    } else {
        Err(WorkspaceError::InvalidState(
            "usage: graft env set <env> <NAME> or graft env set <env> --from-dotenv <file>".into(),
        ))
    }
}

fn env_unset(args: &[String]) -> Result<(), WorkspaceError> {
    if args.len() != 2 {
        return Err(WorkspaceError::InvalidState(
            "usage: graft env unset <env> <NAME>".into(),
        ));
    }
    validate_secret_name(&args[1])?;
    update_bundle_values_mode(&args[0], vec![(args[1].clone(), Vec::new())], false)
}

fn env_grant(args: &[String]) -> Result<(), WorkspaceError> {
    if args.len() != 3 {
        return Err(WorkspaceError::InvalidState(
            "usage: graft env grant <env> <age-recipient> <ed25519-public-hex>".into(),
        ));
    }
    let workspace = Workspace::open(env::current_dir()?)?;
    let credential = load_credential()?;
    let keyring_id = required_ref(&workspace, "secrets/keyring")?;
    let mut keyring = load_keyring(&workspace, keyring_id)?;
    keyring.verify()?;
    let member = MemberIdentity::new(args[1].clone(), parse_public_key(&args[2])?)?;
    if keyring.members.contains_key(&member.member_id()) {
        return Err(WorkspaceError::InvalidState(
            "member is already granted".into(),
        ));
    }
    let identity = keyring.unlock_environment(&args[0], &credential)?;
    keyring.members.insert(member.member_id(), member);
    let old_generation = keyring.environment(&args[0])?.clone();
    let mut generation = old_generation;
    generation.member_wrap = identity.wrap_for_members(
        keyring
            .members
            .values()
            .map(|member| member.age_recipient().to_owned()),
    )?;
    keyring.environments.insert(args[0].clone(), generation);
    keyring.previous = Some(keyring_id);
    keyring.sign(&credential)?;
    let artifact = keyring.artifact()?;
    let new_id = workspace.store().insert(&artifact)?;
    workspace.store().cas_refs(&[graft_core::RefUpdate {
        name: "secrets/keyring".into(),
        expected: Some(keyring_id),
        new: Some(new_id),
    }])?;
    println!("Granted {} to {}", args[0], args[1]);
    Ok(())
}

fn env_revoke(args: &[String]) -> Result<(), WorkspaceError> {
    if args.len() != 2 {
        return Err(WorkspaceError::InvalidState(
            "usage: graft env revoke <env> <member-id>".into(),
        ));
    }
    let member = args[1]
        .parse::<MemberId>()
        .map_err(|_| WorkspaceError::InvalidState("invalid member id".into()))?;
    let workspace = Workspace::open(env::current_dir()?)?;
    let credential = load_credential()?;
    let keyring = load_keyring(&workspace, required_ref(&workspace, "secrets/keyring")?)?;
    if keyring.signer == member {
        return Err(WorkspaceError::InvalidState(
            "the active signer cannot revoke itself".into(),
        ));
    }
    rotate_environment(&workspace, &credential, &args[0], Some(member))?;
    println!("Revoked {member}; environment rotated");
    Ok(())
}

fn env_rotate(args: &[String]) -> Result<(), WorkspaceError> {
    if args.len() != 1 {
        return Err(WorkspaceError::InvalidState(
            "usage: graft env rotate <env>".into(),
        ));
    }
    let workspace = Workspace::open(env::current_dir()?)?;
    let credential = load_credential()?;
    rotate_environment(&workspace, &credential, &args[0], None)?;
    println!("Rotated {}", args[0]);
    Ok(())
}

fn env_run(args: &[String]) -> Result<(), WorkspaceError> {
    let separator = args.iter().position(|arg| arg == "--").ok_or_else(|| {
        WorkspaceError::InvalidState("usage: graft env run <env> -- <command>".into())
    })?;
    if separator != 1 || separator + 1 >= args.len() {
        return Err(WorkspaceError::InvalidState(
            "usage: graft env run <env> -- <command>".into(),
        ));
    }
    let workspace = Workspace::open(env::current_dir()?)?;
    let credential = load_credential()?;
    let keyring = load_keyring(&workspace, required_ref(&workspace, "secrets/keyring")?)?;
    let identity = keyring.unlock_environment(&args[0], &credential)?;
    let bundle_id = required_ref(&workspace, &format!("secrets/{}", args[0]))?;
    let secrets = load_bundle(&workspace, bundle_id)?.decrypt(&identity)?;
    let mut command = process::Command::new(&args[separator + 1]);
    command.args(&args[separator + 2..]);
    for name in secrets.names() {
        let value = secrets.get(name).expect("name exists");
        let value = String::from_utf8(value.to_vec()).map_err(|_| {
            WorkspaceError::InvalidState(format!("secret `{name}` cannot be injected"))
        })?;
        command.env(name, value);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(WorkspaceError::Io(command.exec()))
    }
    #[cfg(not(unix))]
    {
        let status = command.status()?;
        process::exit(status.code().unwrap_or(1));
    }
}

fn update_bundle_values(
    env_name: &str,
    values: Vec<(String, Vec<u8>)>,
) -> Result<(), WorkspaceError> {
    update_bundle_values_mode(env_name, values, true)
}

fn update_bundle_values_mode(
    env_name: &str,
    values: Vec<(String, Vec<u8>)>,
    setting: bool,
) -> Result<(), WorkspaceError> {
    let workspace = Workspace::open(env::current_dir()?)?;
    let credential = load_credential()?;
    let keyring = load_keyring(&workspace, required_ref(&workspace, "secrets/keyring")?)?;
    let identity = keyring.unlock_environment(env_name, &credential)?;
    let reference = format!("secrets/{env_name}");
    let old_id = required_ref(&workspace, &reference)?;
    let mut secrets = load_bundle(&workspace, old_id)?.decrypt(&identity)?;
    let mut existed = false;
    for (name, value) in values {
        if setting {
            existed |= secrets.insert(name, value)?;
        } else {
            existed |= secrets.remove(&name);
        }
    }
    let bundle = SecretBundle::encrypt(&identity, &secrets)?;
    let artifact = bundle.artifact()?;
    let new_id = workspace.store().insert(&artifact)?;
    workspace.store().cas_refs(&[graft_core::RefUpdate {
        name: reference,
        expected: Some(old_id),
        new: Some(new_id),
    }])?;
    if setting {
        println!("{}", if existed { "updated" } else { "created" });
    } else {
        println!("{}", if existed { "removed" } else { "not found" });
    }
    Ok(())
}

fn rotate_environment(
    workspace: &Workspace,
    credential: &Credential,
    name: &str,
    removed: Option<MemberId>,
) -> Result<(), WorkspaceError> {
    let old_keyring_id = required_ref(workspace, "secrets/keyring")?;
    let mut keyring = load_keyring(workspace, old_keyring_id)?;
    let old_generation = keyring.environment(name)?.clone();
    let old_identity = keyring.unlock_environment(name, credential)?;
    let bundle_ref = format!("secrets/{name}");
    let old_bundle_id = required_ref(workspace, &bundle_ref)?;
    let secrets = load_bundle(workspace, old_bundle_id)?.decrypt(&old_identity)?;
    if let Some(member) = removed {
        if keyring.members.remove(&member).is_none() {
            return Err(WorkspaceError::InvalidState("member is not present".into()));
        }
    }
    let next = EnvironmentGeneration {
        generation: old_generation.generation + 1,
        member_wrap: Vec::new(),
        history_links: old_generation.history_links.clone(),
    };
    let identity = graft_core::secrets::EnvironmentIdentity::generate(
        keyring.secrets_domain,
        name.to_owned(),
        next.generation,
    )?;
    let mut next = next;
    next.member_wrap = identity.wrap_for_members(
        keyring
            .members
            .values()
            .map(|member| member.age_recipient().to_owned()),
    )?;
    next.history_links.insert(
        old_generation.generation,
        identity.history_link(&old_identity)?,
    );
    keyring.environments.insert(name.to_owned(), next);
    keyring.previous = Some(old_keyring_id);
    keyring.sign(credential)?;
    let bundle = SecretBundle::encrypt(&identity, &secrets)?;
    let keyring_artifact = keyring.artifact()?;
    let bundle_artifact = bundle.artifact()?;
    let new_keyring_id = workspace.store().insert(&keyring_artifact)?;
    let new_bundle_id = workspace.store().insert(&bundle_artifact)?;
    workspace.store().cas_refs(&[
        graft_core::RefUpdate {
            name: "secrets/keyring".into(),
            expected: Some(old_keyring_id),
            new: Some(new_keyring_id),
        },
        graft_core::RefUpdate {
            name: bundle_ref,
            expected: Some(old_bundle_id),
            new: Some(new_bundle_id),
        },
    ])?;
    Ok(())
}

fn auth_export(args: &[String]) -> Result<(), WorkspaceError> {
    let yes = args.iter().any(|arg| arg == "--yes");
    if !yes && !io::stdin().is_terminal() {
        return Err(WorkspaceError::InvalidState(
            "export requires --yes in scripts; refusing off-TTY prompt".into(),
        ));
    }
    if !yes {
        eprint!("Export credential? [y/N] ");
        io::stderr().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            return Ok(());
        }
    }
    let credential = load_credential()?;
    let target = args
        .iter()
        .find(|arg| arg.as_str() != "--yes")
        .map(PathBuf::from);
    if let Some(path) = target {
        credential.save_atomic(path)?;
    } else {
        io::stdout().write_all(&credential.to_bytes())?;
        io::stdout().flush()?;
    }
    Ok(())
}

fn auth_import(args: &[String]) -> Result<(), WorkspaceError> {
    let path = args
        .first()
        .ok_or_else(|| WorkspaceError::InvalidState("usage: graft auth import <path>".into()))?;
    let credential = Credential::from_bytes(&fs::read(path)?)?;
    let destination = Credential::default_path()?;
    credential.save_atomic(destination.clone())?;
    println!("Imported credential to {}", destination.display());
    Ok(())
}

fn load_credential() -> Result<Credential, WorkspaceError> {
    let path = Credential::default_path()?;
    let (credential, created) = Credential::load_or_create(&path)?;
    if created {
        eprintln!("Created portable Graft credential at {}", path.display());
        eprintln!(
            "Joint member fingerprint: {}",
            credential.member_identity().member_id()
        );
    }
    Ok(credential)
}

fn ensure_secret_domain(workspace: &Workspace) -> Result<[u8; 32], WorkspaceError> {
    let path = workspace.root().join(".graft").join("secrets-domain");
    if let Ok(bytes) = fs::read(&path) {
        let text = String::from_utf8(bytes)
            .map_err(|_| WorkspaceError::InvalidState("invalid secrets domain".into()))?;
        return Ok(text
            .parse::<MemberId>()
            .map_err(|_| WorkspaceError::InvalidState("invalid secrets domain".into()))?
            .as_bytes());
    }
    let domain: [u8; 32] = rand::random();
    fs::write(&path, MemberId::from_bytes(domain).to_string())?;
    Ok(domain)
}

fn required_ref(
    workspace: &Workspace,
    name: &str,
) -> Result<graft_core::ArtifactId, WorkspaceError> {
    workspace
        .store()
        .get_ref(name)?
        .ok_or_else(|| WorkspaceError::InvalidState(format!("missing ref `{name}`")))
}

fn load_keyring(
    workspace: &Workspace,
    id: graft_core::ArtifactId,
) -> Result<Keyring, WorkspaceError> {
    let artifact = workspace
        .store()
        .get(id)?
        .ok_or_else(|| WorkspaceError::InvalidState("keyring artifact is missing".into()))?;
    Ok(Keyring::from_artifact(artifact)?)
}

fn load_bundle(
    workspace: &Workspace,
    id: graft_core::ArtifactId,
) -> Result<SecretBundle, WorkspaceError> {
    let artifact = workspace
        .store()
        .get(id)?
        .ok_or_else(|| WorkspaceError::InvalidState("secret bundle artifact is missing".into()))?;
    Ok(SecretBundle::from_artifact(artifact)?)
}

fn parse_public_key(value: &str) -> Result<[u8; 32], WorkspaceError> {
    Ok(value
        .parse::<MemberId>()
        .map_err(|_| {
            WorkspaceError::InvalidState(
                "Ed25519 public key must be 64 hexadecimal characters".into(),
            )
        })?
        .as_bytes())
}

fn read_secret_value() -> Result<Vec<u8>, WorkspaceError> {
    if io::stdin().is_terminal() {
        eprint!("Value: ");
        io::stderr().flush()?;
        let mut value = String::new();
        io::stdin().read_line(&mut value)?;
        while value.ends_with(['\n', '\r']) {
            value.pop();
        }
        Ok(value.into_bytes())
    } else {
        let mut value = Vec::new();
        io::stdin().read_to_end(&mut value)?;
        Ok(value)
    }
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
