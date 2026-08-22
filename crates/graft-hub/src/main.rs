use std::{
    collections::BTreeSet,
    fs,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use graft_core::{
    artifact::{ChangeId, Revision},
    diff::interdiff,
    land::{discover_stack, land_stack},
    store::StoreError,
    ArtifactId, Person, RepoStore,
};
use graft_sync::{
    apply_file_frames, apply_ref_batch, decode_frames, encode_frames, plan_files, Frame, SyncError,
    ViewRef,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone)]
struct AppState {
    config: Arc<HubConfig>,
}

#[derive(Clone, Debug)]
struct HubConfig {
    data_dir: PathBuf,
    bind: String,
    open_registration: bool,
    max_artifact_bytes: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
enum HubError {
    #[error("database error")]
    Database(#[from] rusqlite::Error),
    #[error("I/O error")]
    Io(#[from] std::io::Error),
    #[error("invalid request")]
    InvalidRequest,
    #[error("authentication required")]
    Unauthorized,
    #[error("not found")]
    NotFound,
    #[error("forbidden")]
    Forbidden,
    #[error("conflict")]
    Conflict,
    #[error("password verification failed")]
    Password,
    #[error("sync error")]
    Sync(#[from] SyncError),
    #[error("store error")]
    Store(#[from] StoreError),
    #[error("land error")]
    Land(#[from] graft_core::LandError),
    #[error("diff error")]
    Diff(#[from] graft_core::diff::DiffError),
}

struct ApiFailure(HubError);

impl From<HubError> for ApiFailure {
    fn from(value: HubError) -> Self {
        Self(value)
    }
}

impl From<rusqlite::Error> for ApiFailure {
    fn from(value: rusqlite::Error) -> Self {
        Self(HubError::Database(value))
    }
}

impl From<std::io::Error> for ApiFailure {
    fn from(value: std::io::Error) -> Self {
        Self(HubError::Io(value))
    }
}

impl From<StoreError> for ApiFailure {
    fn from(value: StoreError) -> Self {
        Self(HubError::Store(value))
    }
}

impl From<graft_core::LandError> for ApiFailure {
    fn from(value: graft_core::LandError) -> Self {
        Self(HubError::Land(value))
    }
}

impl From<graft_core::diff::DiffError> for ApiFailure {
    fn from(value: graft_core::diff::DiffError) -> Self {
        Self(HubError::Diff(value))
    }
}

impl IntoResponse for ApiFailure {
    fn into_response(self) -> Response {
        let message = self.0.to_string();
        let status = match &self.0 {
            HubError::Unauthorized => StatusCode::UNAUTHORIZED,
            HubError::NotFound => StatusCode::NOT_FOUND,
            HubError::Forbidden => StatusCode::FORBIDDEN,
            HubError::Conflict => StatusCode::CONFLICT,
            HubError::InvalidRequest | HubError::Password => StatusCode::BAD_REQUEST,
            HubError::Land(_) => StatusCode::CONFLICT,
            HubError::Database(_)
            | HubError::Io(_)
            | HubError::Sync(_)
            | HubError::Store(_)
            | HubError::Diff(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

#[derive(Deserialize)]
struct RegisterRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct RegisterResponse {
    id: i64,
    username: String,
    admin: bool,
}

#[derive(Deserialize)]
struct TokenRequest {
    username: String,
    password: String,
    name: String,
    mode: Option<String>,
}

#[derive(Serialize)]
struct TokenResponse {
    token: String,
    name: String,
    mode: String,
}

#[derive(Deserialize)]
struct RepoRequest {
    name: String,
    #[serde(rename = "public")]
    is_public: bool,
}

#[derive(Serialize)]
struct RepoResponse {
    name: String,
    owner: String,
    #[serde(rename = "public")]
    is_public: bool,
}

#[derive(Deserialize)]
struct CollaboratorRequest {
    username: String,
    mode: String,
}

#[derive(Deserialize)]
struct MemberPublication {
    member_id: String,
    age_recipient: String,
    signing_public: String,
}

#[derive(Deserialize)]
struct ProposalRequest {
    source: String,
    target: Option<String>,
    title: Option<String>,
}

#[derive(Deserialize)]
struct BodyRequest {
    body: String,
}

#[derive(Deserialize)]
struct ThreadRequest {
    revision: String,
    path: String,
    line: Option<u64>,
    body: String,
}

#[derive(Deserialize)]
struct ResolveRequest {
    resolved: bool,
}

#[derive(Deserialize)]
struct ApprovalRequest {
    revision: Option<String>,
}

#[derive(Deserialize)]
struct IssueRequest {
    title: String,
    body: String,
}

#[derive(Deserialize)]
struct StateRequest {
    state: String,
}

#[derive(Serialize)]
struct ProposalSummary {
    id: i64,
    title: String,
    status: String,
    source: String,
    target: String,
    source_head: String,
    created_at: i64,
    updated_at: i64,
}

struct AuthUser {
    id: i64,
    mode: String,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("graft-hub: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), HubError> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args
        .iter()
        .any(|arg| arg == "--version" || arg == "-V" || arg == "version")
    {
        println!("graft-hub {VERSION}");
        return Ok(());
    }
    let config = parse_config(&args)?;
    initialize_database(&config)?;
    let bind: SocketAddr = config.bind.parse().map_err(|_| HubError::InvalidRequest)?;
    let state = AppState {
        config: Arc::new(config),
    };
    let router = router(state);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    println!("Graft Hub listening on http://{bind}");
    axum::serve(listener, router)
        .await
        .map_err(|_| HubError::Io(std::io::Error::other("server stopped")))
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/register", post(register))
        .route("/api/tokens", post(create_token))
        .route("/api/repos", get(list_repos).post(create_repo))
        .route("/api/repos/{name}", get(repo_info))
        .route("/api/repos/{name}/collaborators", post(add_collaborator))
        .route(
            "/api/repos/{name}/proposals",
            get(list_proposals).post(create_proposal),
        )
        .route("/api/repos/{name}/proposals/{id}", get(proposal_info))
        .route(
            "/api/repos/{name}/proposals/{id}/comments",
            post(add_proposal_comment),
        )
        .route("/api/repos/{name}/proposals/{id}/threads", post(add_thread))
        .route(
            "/api/repos/{name}/proposals/{id}/threads/{thread_id}/resolve",
            post(resolve_thread),
        )
        .route(
            "/api/repos/{name}/proposals/{id}/approvals",
            post(add_approval),
        )
        .route(
            "/api/repos/{name}/proposals/{id}/interdiff/{change}",
            get(proposal_interdiff),
        )
        .route("/api/repos/{name}/proposals/{id}/land", post(land_proposal))
        .route(
            "/api/repos/{name}/proposals/{id}/close",
            post(close_proposal),
        )
        .route(
            "/api/repos/{name}/proposals/{id}/reopen",
            post(reopen_proposal),
        )
        .route(
            "/api/repos/{name}/issues",
            get(list_issues).post(create_issue),
        )
        .route("/api/repos/{name}/issues/{number}", get(issue_info))
        .route(
            "/api/repos/{name}/issues/{number}/comments",
            post(add_issue_comment),
        )
        .route(
            "/api/repos/{name}/issues/{number}/state",
            post(set_issue_state),
        )
        .route("/api/auth/member", post(publish_member))
        .route("/repo/{name}/sync", post(sync))
        .route("/settings/tokens/new", get(token_page))
        .with_state(state)
}

fn parse_config(args: &[String]) -> Result<HubConfig, HubError> {
    let data_dir = std::env::var_os("GRAFT_HUB_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".graft-hub"));
    let bind = std::env::var("GRAFT_HUB_BIND").unwrap_or_else(|_| "127.0.0.1:7676".into());
    let open_registration = std::env::var("GRAFT_HUB_OPEN_REGISTRATION")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let max_artifact_bytes = std::env::var("GRAFT_HUB_MAX_ARTIFACT_BYTES")
        .ok()
        .map(|value| value.parse().map_err(|_| HubError::InvalidRequest))
        .transpose()?;
    let mut config = HubConfig {
        data_dir,
        bind,
        open_registration,
        max_artifact_bytes,
    };
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--data-dir" => {
                index += 1;
                config.data_dir = PathBuf::from(args.get(index).ok_or(HubError::InvalidRequest)?);
            }
            "--bind" => {
                index += 1;
                config.bind = args.get(index).cloned().ok_or(HubError::InvalidRequest)?;
            }
            "--open-registration" => config.open_registration = true,
            "--max-artifact-bytes" => {
                index += 1;
                config.max_artifact_bytes = Some(
                    args.get(index)
                        .ok_or(HubError::InvalidRequest)?
                        .parse()
                        .map_err(|_| HubError::InvalidRequest)?,
                );
            }
            "--help" | "-h" => {
                println!("graft-hub {VERSION}\n\nUsage: graft-hub [--data-dir PATH] [--bind ADDR] [--open-registration] [--max-artifact-bytes N]");
                std::process::exit(0);
            }
            _ => return Err(HubError::InvalidRequest),
        }
        index += 1;
    }
    Ok(config)
}

fn db_path(config: &HubConfig) -> PathBuf {
    config.data_dir.join("hub.db")
}
fn open_db(config: &HubConfig) -> Result<Connection, HubError> {
    Ok(Connection::open(db_path(config))?)
}

fn initialize_database(config: &HubConfig) -> Result<(), HubError> {
    fs::create_dir_all(config.data_dir.join("repos"))?;
    let connection = open_db(config)?;
    connection.execute_batch(
        "PRAGMA foreign_keys = ON;
         CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY, username TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL, admin INTEGER NOT NULL DEFAULT 0);
         CREATE TABLE IF NOT EXISTS pats (id INTEGER PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id), name TEXT NOT NULL, token_hash BLOB NOT NULL UNIQUE, mode TEXT NOT NULL, revoked INTEGER NOT NULL DEFAULT 0);
         CREATE TABLE IF NOT EXISTS repos (name TEXT PRIMARY KEY, owner_id INTEGER NOT NULL REFERENCES users(id), is_public INTEGER NOT NULL, path TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS collaborators (repo_name TEXT NOT NULL REFERENCES repos(name), user_id INTEGER NOT NULL REFERENCES users(id), mode TEXT NOT NULL, PRIMARY KEY(repo_name, user_id));
         CREATE TABLE IF NOT EXISTS profiles (user_id INTEGER PRIMARY KEY REFERENCES users(id), member_id TEXT NOT NULL, age_recipient TEXT NOT NULL, signing_public TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS proposals (
             id INTEGER PRIMARY KEY,
             repo_name TEXT NOT NULL REFERENCES repos(name),
             author_id INTEGER NOT NULL REFERENCES users(id),
             title TEXT NOT NULL,
             source_ref TEXT NOT NULL,
             target_ref TEXT NOT NULL,
             source_head BLOB NOT NULL,
             target_head BLOB,
             status TEXT NOT NULL,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS proposal_changes (
             proposal_id INTEGER NOT NULL REFERENCES proposals(id),
             position INTEGER NOT NULL,
             change_id TEXT NOT NULL,
             revision_id BLOB NOT NULL,
             PRIMARY KEY(proposal_id, position)
         );
         CREATE TABLE IF NOT EXISTS proposal_revision_history (
             proposal_id INTEGER NOT NULL REFERENCES proposals(id),
             change_id TEXT NOT NULL,
             revision_id BLOB NOT NULL,
             seen_at INTEGER NOT NULL,
             PRIMARY KEY(proposal_id, change_id, revision_id)
         );
         CREATE TABLE IF NOT EXISTS proposal_comments (
             id INTEGER PRIMARY KEY,
             proposal_id INTEGER NOT NULL REFERENCES proposals(id),
             author_id INTEGER NOT NULL REFERENCES users(id),
             body TEXT NOT NULL,
             created_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS proposal_threads (
             id INTEGER PRIMARY KEY,
             proposal_id INTEGER NOT NULL REFERENCES proposals(id),
             author_id INTEGER NOT NULL REFERENCES users(id),
             revision_id BLOB NOT NULL,
             path TEXT NOT NULL,
             line INTEGER,
             body TEXT NOT NULL,
             resolved INTEGER NOT NULL DEFAULT 0,
             outdated INTEGER NOT NULL DEFAULT 0,
             created_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS proposal_approvals (
             proposal_id INTEGER NOT NULL REFERENCES proposals(id),
             user_id INTEGER NOT NULL REFERENCES users(id),
             revision_id BLOB,
             created_at INTEGER NOT NULL,
             PRIMARY KEY(proposal_id, user_id)
         );
         CREATE TABLE IF NOT EXISTS issues (
             repo_name TEXT NOT NULL REFERENCES repos(name),
             number INTEGER NOT NULL,
             author_id INTEGER NOT NULL REFERENCES users(id),
             title TEXT NOT NULL,
             body TEXT NOT NULL,
             state TEXT NOT NULL,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL,
             PRIMARY KEY(repo_name, number)
         );
         CREATE TABLE IF NOT EXISTS issue_comments (
             id INTEGER PRIMARY KEY,
             repo_name TEXT NOT NULL,
             issue_number INTEGER NOT NULL,
             author_id INTEGER NOT NULL REFERENCES users(id),
             body TEXT NOT NULL,
             created_at INTEGER NOT NULL,
             FOREIGN KEY(repo_name, issue_number) REFERENCES issues(repo_name, number)
         );",
    )?;
    Ok(())
}

async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "version": VERSION,
        "max_artifact_bytes": state.config.max_artifact_bytes
    }))
}

async fn register(
    State(state): State<AppState>,
    Json(request): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, ApiFailure> {
    if request.username.is_empty() || request.password.is_empty() {
        return Err(ApiFailure(HubError::InvalidRequest));
    }
    let connection = open_db(&state.config)?;
    let transaction = connection.unchecked_transaction()?;
    let count: i64 = transaction.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))?;
    if count > 0 && !state.config.open_registration {
        return Err(ApiFailure(HubError::Forbidden));
    }
    let hash = hash_password(&request.password).map_err(|_| HubError::Password)?;
    let admin = count == 0;
    transaction
        .execute(
            "INSERT INTO users (username, password_hash, admin) VALUES (?1, ?2, ?3)",
            params![request.username, hash, admin as i64],
        )
        .map_err(|error| {
            if error.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
                HubError::Conflict
            } else {
                HubError::Database(error)
            }
        })?;
    let id = transaction.last_insert_rowid();
    transaction.commit()?;
    Ok(Json(RegisterResponse {
        id,
        username: request.username,
        admin,
    }))
}

async fn create_token(
    State(state): State<AppState>,
    Json(request): Json<TokenRequest>,
) -> Result<Json<TokenResponse>, ApiFailure> {
    let mode = request.mode.unwrap_or_else(|| "rw".into());
    if mode != "ro" && mode != "rw" {
        return Err(ApiFailure(HubError::InvalidRequest));
    }
    let connection = open_db(&state.config)?;
    let (user_id, password_hash): (i64, String) = connection
        .query_row(
            "SELECT id, password_hash FROM users WHERE username = ?1",
            params![request.username],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or(HubError::Unauthorized)?;
    verify_password(&request.password, &password_hash).map_err(|_| HubError::Unauthorized)?;
    let token = format!("graft_pat_{}", random_hex::<32>());
    let hash = blake3::hash(token.as_bytes());
    connection.execute(
        "INSERT INTO pats (user_id, name, token_hash, mode) VALUES (?1, ?2, ?3, ?4)",
        params![user_id, request.name, hash.as_bytes().as_slice(), mode],
    )?;
    Ok(Json(TokenResponse {
        token,
        name: request.name,
        mode,
    }))
}

async fn list_repos(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<RepoResponse>>, ApiFailure> {
    let user = auth_user(&state, &headers)?;
    let connection = open_db(&state.config)?;
    let mut statement = connection.prepare("SELECT r.name, u.username, r.is_public FROM repos r JOIN users u ON u.id = r.owner_id WHERE r.is_public = 1 OR r.owner_id = ?1 OR EXISTS (SELECT 1 FROM collaborators c WHERE c.repo_name = r.name AND c.user_id = ?1)")?;
    let rows = statement.query_map(params![user.id], |row| {
        Ok(RepoResponse {
            name: row.get(0)?,
            owner: row.get(1)?,
            is_public: row.get::<_, i64>(2)? != 0,
        })
    })?;
    Ok(Json(rows.collect::<Result<Vec<_>, _>>()?))
}

async fn create_repo(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RepoRequest>,
) -> Result<Json<RepoResponse>, ApiFailure> {
    let user = auth_user(&state, &headers)?;
    require_rw(&user)?;
    validate_repo_name(&request.name)?;
    let connection = open_db(&state.config)?;
    let path = state
        .config
        .data_dir
        .join("repos")
        .join(format!("{}.sqlite", request.name));
    RepoStore::open(&path)
        .map_err(|_| HubError::Database(rusqlite::Error::InvalidPath(path.clone())))?;
    let username: String = connection.query_row(
        "SELECT username FROM users WHERE id = ?1",
        params![user.id],
        |row| row.get(0),
    )?;
    connection
        .execute(
            "INSERT INTO repos (name, owner_id, is_public, path) VALUES (?1, ?2, ?3, ?4)",
            params![
                request.name,
                user.id,
                request.is_public as i64,
                path.to_string_lossy().to_string()
            ],
        )
        .map_err(|error| {
            if error.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
                HubError::Conflict
            } else {
                HubError::Database(error)
            }
        })?;
    Ok(Json(RepoResponse {
        name: request.name,
        owner: username,
        is_public: request.is_public,
    }))
}

async fn repo_info(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<Json<RepoResponse>, ApiFailure> {
    let connection = open_db(&state.config)?;
    let repo = load_repo(&connection, &name)?.ok_or(HubError::NotFound)?;
    if !repo.is_public
        && !can_read(
            &connection,
            &repo,
            auth_user_optional(&state, &headers)?.as_ref(),
        )?
    {
        return Err(ApiFailure(HubError::NotFound));
    }
    let owner: String = connection.query_row(
        "SELECT username FROM users WHERE id = ?1",
        params![repo.owner_id],
        |row| row.get(0),
    )?;
    Ok(Json(RepoResponse {
        name: repo.name,
        owner,
        is_public: repo.is_public,
    }))
}

async fn add_collaborator(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Json(request): Json<CollaboratorRequest>,
) -> Result<Json<serde_json::Value>, ApiFailure> {
    let user = auth_user(&state, &headers)?;
    require_rw(&user)?;
    if request.mode != "ro" && request.mode != "rw" {
        return Err(ApiFailure(HubError::InvalidRequest));
    }
    let connection = open_db(&state.config)?;
    let repo = load_repo(&connection, &name)?.ok_or(HubError::NotFound)?;
    if repo.owner_id != user.id {
        return Err(ApiFailure(HubError::Forbidden));
    }
    let target: i64 = connection
        .query_row(
            "SELECT id FROM users WHERE username = ?1",
            params![request.username],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(HubError::NotFound)?;
    connection.execute("INSERT INTO collaborators (repo_name, user_id, mode) VALUES (?1, ?2, ?3) ON CONFLICT(repo_name, user_id) DO UPDATE SET mode = excluded.mode", params![name, target, request.mode])?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn publish_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(publication): Json<MemberPublication>,
) -> Result<Json<serde_json::Value>, ApiFailure> {
    let user = auth_user(&state, &headers)?;
    if publication.member_id.is_empty()
        || publication.age_recipient.is_empty()
        || publication.signing_public.is_empty()
    {
        return Err(ApiFailure(HubError::InvalidRequest));
    }
    let connection = open_db(&state.config)?;
    connection.execute("INSERT INTO profiles (user_id, member_id, age_recipient, signing_public) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(user_id) DO UPDATE SET member_id = excluded.member_id, age_recipient = excluded.age_recipient, signing_public = excluded.signing_public", params![user.id, publication.member_id, publication.age_recipient, publication.signing_public])?;
    Ok(Json(
        serde_json::json!({ "published": true, "member_id": publication.member_id }),
    ))
}

async fn sync(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiFailure> {
    let frames = decode_frames(&body).map_err(HubError::from)?;
    let connection = open_db(&state.config)?;
    let repo = load_repo(&connection, &name)?.ok_or(HubError::NotFound)?;
    let write = frames
        .iter()
        .any(|frame| matches!(frame, Frame::File { .. } | Frame::RefBatch(_)));
    let user = if write {
        Some(auth_user(&state, &headers)?)
    } else {
        auth_user_optional(&state, &headers)?
    };
    if write {
        require_rw(user.as_ref().ok_or(HubError::Unauthorized)?)?;
    }
    if !repo.is_public && !can_read(&connection, &repo, user.as_ref())? {
        return Err(ApiFailure(HubError::NotFound));
    }
    if write
        && !can_write(
            &connection,
            &repo,
            user.as_ref().ok_or(HubError::Unauthorized)?,
        )?
    {
        return Err(ApiFailure(HubError::Forbidden));
    }
    let store = RepoStore::open(&repo.path)
        .map_err(|_| HubError::Database(rusqlite::Error::InvalidPath(repo.path.clone())))?;
    let max = state.config.max_artifact_bytes;
    let client_ids = frames
        .iter()
        .find_map(|frame| {
            if let Frame::Igot(ids) = frame {
                Some(ids.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();
    if let Err(error) = apply_file_frames(&store, &frames, max) {
        return sync_error(error);
    }
    if let Some(batch) = frames.iter().find_map(|frame| {
        if let Frame::RefBatch(updates) = frame {
            Some(updates.clone())
        } else {
            None
        }
    }) {
        if let Err(error) = apply_ref_batch(&store, batch) {
            return sync_error(error);
        }
        refresh_proposals(&connection, &name, &store)?;
    }
    let local = client_ids.into_iter().collect::<BTreeSet<_>>();
    let requested = store
        .ids()?
        .into_iter()
        .filter(|id| !local.contains(id))
        .collect::<Vec<_>>();
    let mut response_frames = plan_files(&store, &requested, max).map_err(HubError::from)?;
    let refs = store
        .refs()?
        .into_iter()
        .map(|(name, target)| ViewRef { name, target })
        .collect();
    response_frames.push(Frame::View(refs));
    let encoded = encode_frames(&response_frames).map_err(HubError::from)?;
    Ok((
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "application/octet-stream")],
        encoded,
    )
        .into_response())
}

fn sync_error(error: SyncError) -> Result<Response, ApiFailure> {
    let body = encode_frames(&[Frame::Error {
        code: 409,
        message: error.to_string(),
    }])
    .map_err(HubError::from)?;
    Ok((
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "application/octet-stream")],
        body,
    )
        .into_response())
}

#[derive(Clone)]
struct ProposalRecord {
    id: i64,
    title: String,
    source_ref: String,
    target_ref: String,
    source_head: ArtifactId,
    target_head: Option<ArtifactId>,
    status: String,
    created_at: i64,
    updated_at: i64,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn hub_person() -> Person {
    Person {
        name: "Graft Hub".into(),
        email: "hub@graft.local".into(),
        timestamp: now(),
    }
}

fn parse_artifact_id(value: &str) -> Result<ArtifactId, HubError> {
    value.parse().map_err(|_| HubError::InvalidRequest)
}

fn blob_artifact_id(value: Vec<u8>) -> Result<ArtifactId, HubError> {
    let bytes: [u8; 32] = value.try_into().map_err(|_| HubError::Conflict)?;
    Ok(ArtifactId::from_bytes(bytes))
}

fn optional_blob_artifact_id(value: Option<Vec<u8>>) -> Result<Option<ArtifactId>, HubError> {
    value.map(blob_artifact_id).transpose()
}

fn validate_ref_name(name: &str) -> Result<(), HubError> {
    if name.is_empty() || name.contains('\0') || name.starts_with('/') {
        Err(HubError::InvalidRequest)
    } else {
        Ok(())
    }
}

fn load_proposal(
    connection: &Connection,
    repo_name: &str,
    id: i64,
) -> Result<Option<ProposalRecord>, HubError> {
    Ok(connection
        .query_row(
            "SELECT id, title, source_ref, target_ref, source_head, target_head, status, created_at, updated_at FROM proposals WHERE repo_name = ?1 AND id = ?2",
            params![repo_name, id],
            |row| {
                Ok(ProposalRecord {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    source_ref: row.get(2)?,
                    target_ref: row.get(3)?,
                    source_head: blob_artifact_id(row.get(4)?).map_err(|_| rusqlite::Error::InvalidQuery)?,
                    target_head: optional_blob_artifact_id(row.get(5)?)
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    status: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            },
        )
        .optional()?)
}

fn proposal_summary(proposal: &ProposalRecord) -> ProposalSummary {
    ProposalSummary {
        id: proposal.id,
        title: proposal.title.clone(),
        status: proposal.status.clone(),
        source: proposal.source_ref.clone(),
        target: proposal.target_ref.clone(),
        source_head: proposal.source_head.to_string(),
        created_at: proposal.created_at,
        updated_at: proposal.updated_at,
    }
}

fn proposal_context(
    state: &AppState,
    headers: &HeaderMap,
    name: &str,
    id: i64,
    write: bool,
) -> Result<(Connection, RepoRecord, ProposalRecord, Option<AuthUser>), HubError> {
    let connection = open_db(&state.config)?;
    let repo = load_repo(&connection, name)?.ok_or(HubError::NotFound)?;
    let user = if write {
        Some(auth_user(state, headers)?)
    } else {
        auth_user_optional(state, headers)?
    };
    if !can_read(&connection, &repo, user.as_ref())? {
        return Err(HubError::NotFound);
    }
    if write {
        let user = user.as_ref().ok_or(HubError::Unauthorized)?;
        require_rw(user)?;
        if !can_write(&connection, &repo, user)? {
            return Err(HubError::Forbidden);
        }
    }
    let proposal = load_proposal(&connection, name, id)?.ok_or(HubError::NotFound)?;
    Ok((connection, repo, proposal, user))
}

async fn list_proposals(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Vec<ProposalSummary>>, ApiFailure> {
    let connection = open_db(&state.config)?;
    let repo = load_repo(&connection, &name)?.ok_or(HubError::NotFound)?;
    let user = auth_user_optional(&state, &headers)?;
    if !can_read(&connection, &repo, user.as_ref())? {
        return Err(ApiFailure(HubError::NotFound));
    }
    let mut statement = connection.prepare(
        "SELECT id, repo_name, author_id, title, source_ref, target_ref, source_head, target_head, status, created_at, updated_at FROM proposals WHERE repo_name = ?1 ORDER BY id",
    )?;
    let rows = statement.query_map(params![name], |row| {
        Ok(ProposalSummary {
            id: row.get(0)?,
            title: row.get(3)?,
            status: row.get(8)?,
            source: row.get(4)?,
            target: row.get(5)?,
            source_head: blob_artifact_id(row.get(6)?)
                .map_err(|_| rusqlite::Error::InvalidQuery)?
                .to_string(),
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
        })
    })?;
    Ok(Json(rows.collect::<Result<Vec<_>, _>>()?))
}

async fn create_proposal(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Json(request): Json<ProposalRequest>,
) -> Result<Json<ProposalSummary>, ApiFailure> {
    let user = auth_user(&state, &headers)?;
    require_rw(&user)?;
    validate_ref_name(&request.source)?;
    let target = request.target.unwrap_or_else(|| "branches/main".into());
    validate_ref_name(&target)?;
    let connection = open_db(&state.config)?;
    let repo = load_repo(&connection, &name)?.ok_or(HubError::NotFound)?;
    if !can_write(&connection, &repo, &user)? {
        return Err(ApiFailure(HubError::Forbidden));
    }
    let store = RepoStore::open(&repo.path)?;
    let source_head = store
        .get_ref(&request.source)?
        .ok_or(HubError::InvalidRequest)?;
    let target_head = store.get_ref(&target)?;
    let stack = discover_stack(&store, target_head, source_head)?;
    if stack.is_empty() {
        return Err(ApiFailure(HubError::InvalidRequest));
    }
    let timestamp = now();
    let title = request
        .title
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| format!("Proposal from {}", request.source));
    connection.execute(
        "INSERT INTO proposals (repo_name, author_id, title, source_ref, target_ref, source_head, target_head, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'open', ?8, ?8)",
        params![
            name,
            user.id,
            title,
            request.source,
            target,
            source_head.as_slice().as_slice(),
            target_head.map(|id| id.as_bytes().to_vec()),
            timestamp
        ],
    )?;
    let id = connection.last_insert_rowid();
    for (position, change) in stack.iter().enumerate() {
        connection.execute(
            "INSERT INTO proposal_changes (proposal_id, position, change_id, revision_id) VALUES (?1, ?2, ?3, ?4)",
            params![
                id,
                position as i64,
                change.change_id.to_string(),
                change.revision_id.as_slice().as_slice()
            ],
        )?;
        connection.execute(
            "INSERT INTO proposal_revision_history (proposal_id, change_id, revision_id, seen_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                id,
                change.change_id.to_string(),
                change.revision_id.as_slice().as_slice(),
                timestamp
            ],
        )?;
    }
    store.set_ref(&format!("proposals/{id}"), source_head)?;
    let proposal = load_proposal(&connection, &name, id)?.ok_or(HubError::Conflict)?;
    Ok(Json(proposal_summary(&proposal)))
}

async fn proposal_info(
    State(state): State<AppState>,
    Path((name, id)): Path<(String, i64)>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiFailure> {
    let (connection, repo, proposal, _) = proposal_context(&state, &headers, &name, id, false)?;
    let store = RepoStore::open(&repo.path)?;
    let changes = proposal_changes(&connection, id)?;
    let comments = query_proposal_comments(&connection, id)?;
    let threads = query_threads(&connection, id)?;
    let approvals = query_approvals(&connection, id)?;
    let target_current = store.get_ref(&proposal.target_ref)?;
    Ok(Json(serde_json::json!({
        "proposal": proposal_summary(&proposal),
        "target_current": target_current.map(|id| id.to_string()),
        "target_stale": target_current != proposal.target_head,
        "changes": changes,
        "comments": comments,
        "threads": threads,
        "approvals": approvals,
        "landable": proposal.status == "open" && target_current == proposal.target_head
    })))
}

fn proposal_changes(
    connection: &Connection,
    proposal_id: i64,
) -> Result<Vec<serde_json::Value>, HubError> {
    let mut statement = connection.prepare(
        "SELECT position, change_id, revision_id FROM proposal_changes WHERE proposal_id = ?1 ORDER BY position",
    )?;
    let rows = statement.query_map(params![proposal_id], |row| {
        let revision = blob_artifact_id(row.get(2)?).map_err(|_| rusqlite::Error::InvalidQuery)?;
        Ok(serde_json::json!({
            "position": row.get::<_, i64>(0)?,
            "change_id": row.get::<_, String>(1)?,
            "revision": revision.to_string()
        }))
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn query_proposal_comments(
    connection: &Connection,
    proposal_id: i64,
) -> Result<Vec<serde_json::Value>, HubError> {
    let mut statement = connection.prepare(
        "SELECT id, author_id, body, created_at FROM proposal_comments WHERE proposal_id = ?1 ORDER BY id",
    )?;
    let rows = statement.query_map(params![proposal_id], |row| {
        Ok(serde_json::json!({
            "id": row.get::<_, i64>(0)?,
            "author_id": row.get::<_, i64>(1)?,
            "body": row.get::<_, String>(2)?,
            "created_at": row.get::<_, i64>(3)?
        }))
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn query_threads(
    connection: &Connection,
    proposal_id: i64,
) -> Result<Vec<serde_json::Value>, HubError> {
    let mut statement = connection.prepare(
        "SELECT id, author_id, revision_id, path, line, body, resolved, outdated, created_at FROM proposal_threads WHERE proposal_id = ?1 ORDER BY id",
    )?;
    let rows = statement.query_map(params![proposal_id], |row| {
        let revision = blob_artifact_id(row.get(2)?).map_err(|_| rusqlite::Error::InvalidQuery)?;
        Ok(serde_json::json!({
            "id": row.get::<_, i64>(0)?,
            "author_id": row.get::<_, i64>(1)?,
            "revision": revision.to_string(),
            "path": row.get::<_, String>(3)?,
            "line": row.get::<_, Option<i64>>(4)?,
            "body": row.get::<_, String>(5)?,
            "resolved": row.get::<_, i64>(6)? != 0,
            "outdated": row.get::<_, i64>(7)? != 0,
            "created_at": row.get::<_, i64>(8)?
        }))
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn query_approvals(
    connection: &Connection,
    proposal_id: i64,
) -> Result<Vec<serde_json::Value>, HubError> {
    let mut statement = connection.prepare(
        "SELECT user_id, revision_id, created_at FROM proposal_approvals WHERE proposal_id = ?1 ORDER BY user_id",
    )?;
    let rows = statement.query_map(params![proposal_id], |row| {
        let revision = optional_blob_artifact_id(row.get(1)?)
            .map_err(|_| rusqlite::Error::InvalidQuery)?
            .map(|id| id.to_string());
        Ok(serde_json::json!({
            "user_id": row.get::<_, i64>(0)?,
            "revision": revision,
            "created_at": row.get::<_, i64>(2)?
        }))
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

async fn add_proposal_comment(
    State(state): State<AppState>,
    Path((name, id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<BodyRequest>,
) -> Result<Json<serde_json::Value>, ApiFailure> {
    let (connection, _, proposal, user) = proposal_context(&state, &headers, &name, id, true)?;
    let user = user.ok_or(HubError::Unauthorized)?;
    validate_body(&request.body)?;
    let timestamp = now();
    connection.execute(
        "INSERT INTO proposal_comments (proposal_id, author_id, body, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![proposal.id, user.id, request.body, timestamp],
    )?;
    Ok(Json(serde_json::json!({
        "id": connection.last_insert_rowid(),
        "body": request.body,
        "created_at": timestamp
    })))
}

async fn add_thread(
    State(state): State<AppState>,
    Path((name, id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<ThreadRequest>,
) -> Result<Json<serde_json::Value>, ApiFailure> {
    let (connection, repo, proposal, user) = proposal_context(&state, &headers, &name, id, true)?;
    let user = user.ok_or(HubError::Unauthorized)?;
    validate_body(&request.body)?;
    if request.path.is_empty() || request.path.contains('\0') {
        return Err(ApiFailure(HubError::InvalidRequest));
    }
    let revision = parse_artifact_id(&request.revision)?;
    let store = RepoStore::open(&repo.path)?;
    let artifact = store.get(revision)?.ok_or(HubError::NotFound)?;
    let revision_artifact =
        Revision::from_artifact(artifact).map_err(|_| HubError::InvalidRequest)?;
    let change_id = revision_artifact.change_id().to_string();
    let known: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM proposal_changes WHERE proposal_id = ?1 AND change_id = ?2 LIMIT 1",
            params![proposal.id, change_id],
            |row| row.get(0),
        )
        .optional()?;
    if known.is_none() {
        return Err(ApiFailure(HubError::InvalidRequest));
    }
    let timestamp = now();
    connection.execute(
        "INSERT INTO proposal_threads (proposal_id, author_id, revision_id, path, line, body, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            proposal.id,
            user.id,
            revision.as_slice().as_slice(),
            request.path,
            request.line.map(|line| line as i64),
            request.body,
            timestamp
        ],
    )?;
    Ok(Json(serde_json::json!({
        "id": connection.last_insert_rowid(),
        "revision": revision.to_string(),
        "path": request.path,
        "line": request.line,
        "body": request.body,
        "outdated": false
    })))
}

async fn resolve_thread(
    State(state): State<AppState>,
    Path((name, id, thread_id)): Path<(String, i64, i64)>,
    headers: HeaderMap,
    Json(request): Json<ResolveRequest>,
) -> Result<Json<serde_json::Value>, ApiFailure> {
    let (connection, _, proposal, _) = proposal_context(&state, &headers, &name, id, true)?;
    let updated = connection.execute(
        "UPDATE proposal_threads SET resolved = ?1 WHERE id = ?2 AND proposal_id = ?3",
        params![request.resolved as i64, thread_id, proposal.id],
    )?;
    if updated == 0 {
        return Err(ApiFailure(HubError::NotFound));
    }
    Ok(Json(serde_json::json!({
        "id": thread_id,
        "resolved": request.resolved
    })))
}

async fn add_approval(
    State(state): State<AppState>,
    Path((name, id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<ApprovalRequest>,
) -> Result<Json<serde_json::Value>, ApiFailure> {
    let (connection, _, proposal, user) = proposal_context(&state, &headers, &name, id, true)?;
    let user = user.ok_or(HubError::Unauthorized)?;
    let revision = request
        .revision
        .as_deref()
        .map(parse_artifact_id)
        .transpose()?
        .unwrap_or(proposal.source_head);
    connection.execute(
        "INSERT INTO proposal_approvals (proposal_id, user_id, revision_id, created_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(proposal_id, user_id) DO UPDATE SET revision_id = excluded.revision_id, created_at = excluded.created_at",
        params![
            proposal.id,
            user.id,
            revision.as_slice().as_slice(),
            now()
        ],
    )?;
    Ok(Json(serde_json::json!({
        "approved": true,
        "proposal_id": proposal.id,
        "revision": revision.to_string()
    })))
}

async fn proposal_interdiff(
    State(state): State<AppState>,
    Path((name, id, change)): Path<(String, i64, String)>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiFailure> {
    let (connection, repo, proposal, _) = proposal_context(&state, &headers, &name, id, false)?;
    let change_id = change
        .parse::<ChangeId>()
        .map_err(|_| HubError::InvalidRequest)?;
    let mut statement = connection.prepare(
        "SELECT revision_id FROM proposal_revision_history WHERE proposal_id = ?1 AND change_id = ?2 ORDER BY seen_at, revision_id",
    )?;
    let revisions = statement
        .query_map(params![proposal.id, change_id.to_string()], |row| {
            blob_artifact_id(row.get(0)?).map_err(|_| rusqlite::Error::InvalidQuery)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let changes = if revisions.len() < 2 {
        Vec::new()
    } else {
        let store = RepoStore::open(&repo.path)?;
        interdiff(
            &store,
            revisions[revisions.len() - 2],
            revisions[revisions.len() - 1],
        )?
        .into_iter()
        .map(|change| {
            serde_json::json!({
                "path": change.path,
                "kind": format!("{:?}", change.kind)
            })
        })
        .collect()
    };
    Ok(Json(serde_json::json!({
        "change_id": change_id.to_string(),
        "changes": changes
    })))
}

async fn land_proposal(
    State(state): State<AppState>,
    Path((name, id)): Path<(String, i64)>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiFailure> {
    let (connection, repo, proposal, _) = proposal_context(&state, &headers, &name, id, true)?;
    if proposal.status != "open" {
        return Err(ApiFailure(HubError::Conflict));
    }
    let store = RepoStore::open(&repo.path)?;
    let target_current = store.get_ref(&proposal.target_ref)?;
    if target_current != proposal.target_head {
        return Err(ApiFailure(HubError::Land(
            graft_core::LandError::TargetAdvanced,
        )));
    }
    let source = store
        .get_ref(&proposal.source_ref)?
        .ok_or(HubError::Conflict)?;
    connection.execute(
        "UPDATE proposals SET status = 'landing', updated_at = ?1 WHERE id = ?2 AND status = 'open'",
        params![now(), proposal.id],
    )?;
    let plan = match land_stack(
        &store,
        &proposal.target_ref,
        proposal.target_head,
        source,
        hub_person(),
    ) {
        Ok(plan) => plan,
        Err(error) => {
            connection.execute(
                "UPDATE proposals SET status = 'open', updated_at = ?1 WHERE id = ?2",
                params![now(), proposal.id],
            )?;
            return Err(ApiFailure(HubError::Land(error)));
        }
    };
    let proposal_ref = format!("proposals/{}", proposal.id);
    let proposal_current = store.get_ref(&proposal_ref)?;
    if proposal_current.is_some() {
        store.cas_refs(&[graft_core::RefUpdate {
            name: proposal_ref,
            expected: proposal_current,
            new: None,
        }])?;
    }
    connection.execute(
        "UPDATE proposals SET status = 'landed', updated_at = ?1 WHERE id = ?2",
        params![now(), proposal.id],
    )?;
    Ok(Json(serde_json::json!({
        "landed": true,
        "proposal_id": proposal.id,
        "target": plan.tip.map(|id| id.to_string())
    })))
}

async fn close_proposal(
    State(state): State<AppState>,
    Path((name, id)): Path<(String, i64)>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiFailure> {
    let (connection, repo, proposal, _) = proposal_context(&state, &headers, &name, id, true)?;
    if proposal.status != "open" {
        return Err(ApiFailure(HubError::Conflict));
    }
    connection.execute(
        "UPDATE proposals SET status = 'closed', updated_at = ?1 WHERE id = ?2",
        params![now(), proposal.id],
    )?;
    let store = RepoStore::open(&repo.path)?;
    let proposal_ref = format!("proposals/{}", proposal.id);
    if let Some(current) = store.get_ref(&proposal_ref)? {
        store.cas_refs(&[graft_core::RefUpdate {
            name: proposal_ref,
            expected: Some(current),
            new: None,
        }])?;
    }
    Ok(Json(
        serde_json::json!({ "closed": true, "proposal_id": proposal.id }),
    ))
}

async fn reopen_proposal(
    State(state): State<AppState>,
    Path((name, id)): Path<(String, i64)>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiFailure> {
    let (connection, repo, proposal, _) = proposal_context(&state, &headers, &name, id, true)?;
    if proposal.status != "closed" {
        return Err(ApiFailure(HubError::Conflict));
    }
    let store = RepoStore::open(&repo.path)?;
    let source = store
        .get_ref(&proposal.source_ref)?
        .ok_or(HubError::Conflict)?;
    connection.execute(
        "UPDATE proposals SET status = 'open', source_head = ?1, updated_at = ?2 WHERE id = ?3",
        params![source.as_slice().as_slice(), now(), proposal.id],
    )?;
    store.set_ref(&format!("proposals/{}", proposal.id), source)?;
    Ok(Json(
        serde_json::json!({ "reopened": true, "proposal_id": proposal.id }),
    ))
}

fn validate_body(body: &str) -> Result<(), HubError> {
    if body.trim().is_empty() || body.len() > 1024 * 1024 {
        Err(HubError::InvalidRequest)
    } else {
        Ok(())
    }
}

async fn list_issues(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Vec<serde_json::Value>>, ApiFailure> {
    let connection = open_db(&state.config)?;
    let repo = load_repo(&connection, &name)?.ok_or(HubError::NotFound)?;
    let user = auth_user_optional(&state, &headers)?;
    if !can_read(&connection, &repo, user.as_ref())? {
        return Err(ApiFailure(HubError::NotFound));
    }
    let mut statement = connection.prepare(
        "SELECT number, title, state, author_id, created_at, updated_at FROM issues WHERE repo_name = ?1 ORDER BY number",
    )?;
    let rows = statement.query_map(params![name], |row| {
        Ok(serde_json::json!({
            "number": row.get::<_, i64>(0)?,
            "title": row.get::<_, String>(1)?,
            "state": row.get::<_, String>(2)?,
            "author_id": row.get::<_, i64>(3)?,
            "created_at": row.get::<_, i64>(4)?,
            "updated_at": row.get::<_, i64>(5)?
        }))
    })?;
    Ok(Json(rows.collect::<Result<Vec<_>, _>>()?))
}

async fn create_issue(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Json(request): Json<IssueRequest>,
) -> Result<Json<serde_json::Value>, ApiFailure> {
    let user = auth_user(&state, &headers)?;
    require_rw(&user)?;
    validate_body(&request.title)?;
    validate_body(&request.body)?;
    let connection = open_db(&state.config)?;
    let repo = load_repo(&connection, &name)?.ok_or(HubError::NotFound)?;
    if !can_write(&connection, &repo, &user)? {
        return Err(ApiFailure(HubError::Forbidden));
    }
    let number: i64 = connection.query_row(
        "SELECT COALESCE(MAX(number), 0) + 1 FROM issues WHERE repo_name = ?1",
        params![name],
        |row| row.get(0),
    )?;
    let timestamp = now();
    connection.execute(
        "INSERT INTO issues (repo_name, number, author_id, title, body, state, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, 'open', ?6, ?6)",
        params![name, number, user.id, request.title, request.body, timestamp],
    )?;
    Ok(Json(serde_json::json!({
        "number": number,
        "title": request.title,
        "body": request.body,
        "state": "open",
        "created_at": timestamp
    })))
}

async fn issue_info(
    State(state): State<AppState>,
    Path((name, number)): Path<(String, i64)>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiFailure> {
    let connection = open_db(&state.config)?;
    let repo = load_repo(&connection, &name)?.ok_or(HubError::NotFound)?;
    let user = auth_user_optional(&state, &headers)?;
    if !can_read(&connection, &repo, user.as_ref())? {
        return Err(ApiFailure(HubError::NotFound));
    }
    let issue = connection
        .query_row(
            "SELECT number, title, body, state, author_id, created_at, updated_at FROM issues WHERE repo_name = ?1 AND number = ?2",
            params![name, number],
            |row| {
                Ok(serde_json::json!({
                    "number": row.get::<_, i64>(0)?,
                    "title": row.get::<_, String>(1)?,
                    "body": row.get::<_, String>(2)?,
                    "state": row.get::<_, String>(3)?,
                    "author_id": row.get::<_, i64>(4)?,
                    "created_at": row.get::<_, i64>(5)?,
                    "updated_at": row.get::<_, i64>(6)?
                }))
            },
        )
        .optional()?
        .ok_or(HubError::NotFound)?;
    let mut statement = connection.prepare(
        "SELECT id, author_id, body, created_at FROM issue_comments WHERE repo_name = ?1 AND issue_number = ?2 ORDER BY id",
    )?;
    let comments = statement
        .query_map(params![name, number], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, i64>(0)?,
                "author_id": row.get::<_, i64>(1)?,
                "body": row.get::<_, String>(2)?,
                "created_at": row.get::<_, i64>(3)?
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(
        serde_json::json!({ "issue": issue, "comments": comments }),
    ))
}

async fn add_issue_comment(
    State(state): State<AppState>,
    Path((name, number)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<BodyRequest>,
) -> Result<Json<serde_json::Value>, ApiFailure> {
    let user = auth_user(&state, &headers)?;
    require_rw(&user)?;
    validate_body(&request.body)?;
    let connection = open_db(&state.config)?;
    let repo = load_repo(&connection, &name)?.ok_or(HubError::NotFound)?;
    if !can_write(&connection, &repo, &user)? {
        return Err(ApiFailure(HubError::Forbidden));
    }
    let exists: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM issues WHERE repo_name = ?1 AND number = ?2",
            params![name, number],
            |row| row.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return Err(ApiFailure(HubError::NotFound));
    }
    let timestamp = now();
    connection.execute(
        "INSERT INTO issue_comments (repo_name, issue_number, author_id, body, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![name, number, user.id, request.body, timestamp],
    )?;
    Ok(Json(serde_json::json!({
        "id": connection.last_insert_rowid(),
        "body": request.body,
        "created_at": timestamp
    })))
}

async fn set_issue_state(
    State(state): State<AppState>,
    Path((name, number)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<StateRequest>,
) -> Result<Json<serde_json::Value>, ApiFailure> {
    let user = auth_user(&state, &headers)?;
    require_rw(&user)?;
    if request.state != "open" && request.state != "closed" {
        return Err(ApiFailure(HubError::InvalidRequest));
    }
    let connection = open_db(&state.config)?;
    let repo = load_repo(&connection, &name)?.ok_or(HubError::NotFound)?;
    if !can_write(&connection, &repo, &user)? {
        return Err(ApiFailure(HubError::Forbidden));
    }
    let updated = connection.execute(
        "UPDATE issues SET state = ?1, updated_at = ?2 WHERE repo_name = ?3 AND number = ?4",
        params![request.state, now(), name, number],
    )?;
    if updated == 0 {
        return Err(ApiFailure(HubError::NotFound));
    }
    Ok(Json(serde_json::json!({
        "number": number,
        "state": request.state
    })))
}

fn refresh_proposals(
    connection: &Connection,
    repo_name: &str,
    store: &RepoStore,
) -> Result<(), HubError> {
    let mut statement = connection.prepare(
        "SELECT id, source_ref FROM proposals WHERE repo_name = ?1 AND status IN ('open', 'landing')",
    )?;
    let proposals = statement
        .query_map(params![repo_name], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (proposal_id, source_ref) in proposals {
        let Some(source_head) = store.get_ref(&source_ref)? else {
            continue;
        };
        let target_head: Option<ArtifactId> = connection
            .query_row(
                "SELECT target_head FROM proposals WHERE id = ?1",
                params![proposal_id],
                |row| row.get::<_, Option<Vec<u8>>>(0),
            )?
            .map(blob_artifact_id)
            .transpose()?;
        let stack = discover_stack(store, target_head, source_head)?;
        let timestamp = now();
        for change in &stack {
            connection.execute(
                "INSERT OR IGNORE INTO proposal_revision_history (proposal_id, change_id, revision_id, seen_at) VALUES (?1, ?2, ?3, ?4)",
                params![
                    proposal_id,
                    change.change_id.to_string(),
                    change.revision_id.as_slice().as_slice(),
                    timestamp
                ],
            )?;
        }
        connection.execute(
            "DELETE FROM proposal_changes WHERE proposal_id = ?1",
            params![proposal_id],
        )?;
        for (position, change) in stack.iter().enumerate() {
            connection.execute(
                "INSERT INTO proposal_changes (proposal_id, position, change_id, revision_id) VALUES (?1, ?2, ?3, ?4)",
                params![
                    proposal_id,
                    position as i64,
                    change.change_id.to_string(),
                    change.revision_id.as_slice().as_slice()
                ],
            )?;
        }
        connection.execute(
            "UPDATE proposals SET source_head = ?1, updated_at = ?2 WHERE id = ?3",
            params![source_head.as_slice().as_slice(), timestamp, proposal_id],
        )?;
        for thread_id in thread_ids(connection, proposal_id)? {
            let (thread_revision,): (Vec<u8>,) = connection.query_row(
                "SELECT revision_id FROM proposal_threads WHERE id = ?1",
                params![thread_id],
                |row| Ok((row.get(0)?,)),
            )?;
            let outdated = !stack
                .iter()
                .any(|change| change.revision_id.as_slice() == thread_revision.as_slice());
            connection.execute(
                "UPDATE proposal_threads SET outdated = ?1 WHERE id = ?2",
                params![outdated as i64, thread_id],
            )?;
        }
    }
    Ok(())
}

fn thread_ids(connection: &Connection, proposal_id: i64) -> Result<Vec<i64>, HubError> {
    let mut statement =
        connection.prepare("SELECT id FROM proposal_threads WHERE proposal_id = ?1")?;
    let rows = statement
        .query_map(params![proposal_id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

async fn token_page() -> impl IntoResponse {
    (StatusCode::OK, [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")], "<!doctype html><html><head><title>Graft Hub token</title></head><body><main><h1>Create a Graft token</h1><p>Use the account screen to create a named personal access token, then paste it into <code>graft auth login</code>.</p></main></body></html>")
}

#[derive(Clone)]
struct RepoRecord {
    name: String,
    owner_id: i64,
    is_public: bool,
    path: PathBuf,
}

fn load_repo(connection: &Connection, name: &str) -> Result<Option<RepoRecord>, HubError> {
    Ok(connection
        .query_row(
            "SELECT name, owner_id, is_public, path FROM repos WHERE name = ?1",
            params![name],
            |row| {
                Ok(RepoRecord {
                    name: row.get(0)?,
                    owner_id: row.get(1)?,
                    is_public: row.get::<_, i64>(2)? != 0,
                    path: PathBuf::from(row.get::<_, String>(3)?),
                })
            },
        )
        .optional()?)
}

fn auth_user(state: &AppState, headers: &HeaderMap) -> Result<AuthUser, HubError> {
    auth_user_optional(state, headers)?.ok_or(HubError::Unauthorized)
}

fn auth_user_optional(state: &AppState, headers: &HeaderMap) -> Result<Option<AuthUser>, HubError> {
    let Some(value) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return Ok(None);
    };
    let Some(token) = value.strip_prefix("Bearer ") else {
        return Err(HubError::Unauthorized);
    };
    let hash = blake3::hash(token.as_bytes());
    let connection = open_db(&state.config)?;
    Ok(connection
        .query_row(
            "SELECT user_id, mode FROM pats WHERE token_hash = ?1 AND revoked = 0",
            params![hash.as_bytes().as_slice()],
            |row| {
                Ok(AuthUser {
                    id: row.get(0)?,
                    mode: row.get(1)?,
                })
            },
        )
        .optional()?)
}

fn require_rw(user: &AuthUser) -> Result<(), HubError> {
    if user.mode == "rw" {
        Ok(())
    } else {
        Err(HubError::Forbidden)
    }
}

fn can_read(
    connection: &Connection,
    repo: &RepoRecord,
    user: Option<&AuthUser>,
) -> Result<bool, HubError> {
    if repo.is_public || user.is_some_and(|user| user.id == repo.owner_id) {
        return Ok(true);
    }
    let Some(user) = user else {
        return Ok(false);
    };
    Ok(connection
        .query_row(
            "SELECT 1 FROM collaborators WHERE repo_name = ?1 AND user_id = ?2",
            params![repo.name, user.id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn can_write(
    connection: &Connection,
    repo: &RepoRecord,
    user: &AuthUser,
) -> Result<bool, HubError> {
    if repo.owner_id == user.id {
        return Ok(true);
    }
    Ok(connection
        .query_row(
            "SELECT mode FROM collaborators WHERE repo_name = ?1 AND user_id = ?2",
            params![repo.name, user.id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .is_some_and(|mode| mode == "rw"))
}

fn validate_repo_name(name: &str) -> Result<(), HubError> {
    if name.is_empty()
        || name.len() > 128
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
    {
        Err(HubError::InvalidRequest)
    } else {
        Ok(())
    }
}

fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
}

fn verify_password(password: &str, hash: &str) -> Result<(), argon2::password_hash::Error> {
    let parsed = PasswordHash::new(hash)?;
    Argon2::default().verify_password(password.as_bytes(), &parsed)
}

fn random_hex<const N: usize>() -> String {
    let bytes: [u8; N] = rand::random();
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        sync::Arc,
    };

    use axum::{
        body::{to_bytes, Body},
        http::{header, Method, Request, StatusCode},
    };
    use graft_core::{
        tree::{build_tree, FileEntry},
        Blob, ChangeId, Person, RepoStore, Revision, TreeEntryKind,
    };
    use graft_sync::{decode_frames, encode_frames, Direction, Frame, Handshake, WireRefUpdate};
    use serde_json::Value;
    use tower::ServiceExt;

    use super::{initialize_database, router, AppState, HubConfig};

    static NEXT_CONFIG: AtomicU64 = AtomicU64::new(0);

    fn test_config() -> HubConfig {
        let nonce = NEXT_CONFIG.fetch_add(1, Ordering::Relaxed);
        HubConfig {
            data_dir: std::env::temp_dir()
                .join(format!("graft-hub-test-{}-{nonce}", std::process::id())),
            bind: "127.0.0.1:0".into(),
            open_registration: false,
            max_artifact_bytes: None,
        }
    }

    async fn request(
        app: &axum::Router,
        method: Method,
        uri: &str,
        token: Option<&str>,
        body: Vec<u8>,
        content_type: &str,
    ) -> axum::response::Response {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, content_type);
        if let Some(token) = token {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        app.clone()
            .oneshot(builder.body(Body::from(body)).expect("valid request"))
            .await
            .expect("router response")
    }

    async fn json_request(
        app: &axum::Router,
        method: Method,
        uri: &str,
        token: Option<&str>,
        value: Value,
    ) -> axum::response::Response {
        request(
            app,
            method,
            uri,
            token,
            serde_json::to_vec(&value).expect("json encoding"),
            "application/json",
        )
        .await
    }

    async fn body_json(response: axum::response::Response) -> Value {
        let bytes = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("response body");
        serde_json::from_slice(&bytes).expect("json response")
    }

    #[tokio::test]
    async fn api_auth_repo_and_sync_flow() {
        let config = test_config();
        initialize_database(&config).expect("database initializes");
        let app = router(AppState {
            config: Arc::new(config.clone()),
        });

        let response = json_request(
            &app,
            Method::POST,
            "/api/register",
            None,
            serde_json::json!({"username": "ishan", "password": "correct horse"}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);

        let response = json_request(
            &app,
            Method::POST,
            "/api/tokens",
            None,
            serde_json::json!({
                "username": "ishan",
                "password": "correct horse",
                "name": "test",
                "mode": "rw"
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let token = body_json(response)
            .await
            .get("token")
            .and_then(Value::as_str)
            .expect("token")
            .to_owned();

        let response = json_request(
            &app,
            Method::POST,
            "/api/repos",
            Some(&token),
            serde_json::json!({"name": "demo", "public": true}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);

        let blob = Blob::from_bytes(b"hello from hub").expect("blob");
        let frames = encode_frames(&[
            Frame::Handshake(Handshake::v1(Direction::Push)),
            Frame::File {
                id: blob.id(),
                bytes: blob.artifact().bytes().to_vec(),
            },
            Frame::RefBatch(vec![WireRefUpdate {
                name: "heads/main".into(),
                expected: None,
                new: Some(blob.id()),
            }]),
        ])
        .expect("request frames");
        let response = request(
            &app,
            Method::POST,
            "/repo/demo/sync",
            Some(&token),
            frames,
            "application/octet-stream",
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let response_frames = decode_frames(
            &to_bytes(response.into_body(), 1024 * 1024)
                .await
                .expect("sync response"),
        )
        .expect("response frames");
        assert!(response_frames.iter().any(|frame| matches!(
            frame,
            Frame::View(refs) if refs.iter().any(|view| view.name == "heads/main" && view.target == blob.id())
        )));

        let response = request(
            &app,
            Method::POST,
            "/repo/demo/sync",
            None,
            encode_frames(&[Frame::Handshake(Handshake::v1(Direction::Pull))])
                .expect("pull frames"),
            "application/octet-stream",
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);

        let response = json_request(
            &app,
            Method::POST,
            "/api/repos",
            Some(&token),
            serde_json::json!({"name": "private", "public": false}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let response = request(
            &app,
            Method::POST,
            "/repo/private/sync",
            None,
            encode_frames(&[Frame::Handshake(Handshake::v1(Direction::Pull))])
                .expect("pull frames"),
            "application/octet-stream",
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let repo_path: PathBuf = config.data_dir.join("repos/demo.sqlite");
        let store = RepoStore::open(repo_path).expect("repo store");
        assert_eq!(
            store.get_ref("heads/main").expect("ref lookup"),
            Some(blob.id())
        );
        std::fs::remove_dir_all(config.data_dir).expect("test cleanup");
    }

    #[tokio::test]
    async fn proposal_review_and_land_flow() {
        let config = test_config();
        initialize_database(&config).expect("database initializes");
        let app = router(AppState {
            config: Arc::new(config.clone()),
        });
        let response = json_request(
            &app,
            Method::POST,
            "/api/register",
            None,
            serde_json::json!({"username": "owner", "password": "correct horse"}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let response = json_request(
            &app,
            Method::POST,
            "/api/tokens",
            None,
            serde_json::json!({
                "username": "owner",
                "password": "correct horse",
                "name": "test",
                "mode": "rw"
            }),
        )
        .await;
        let token = body_json(response)
            .await
            .get("token")
            .and_then(Value::as_str)
            .expect("token")
            .to_owned();
        let response = json_request(
            &app,
            Method::POST,
            "/api/repos",
            Some(&token),
            serde_json::json!({"name": "review", "public": false}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);

        let store = RepoStore::open(config.data_dir.join("repos/review.sqlite")).expect("store");
        let person = Person {
            name: "owner".into(),
            email: "owner@example.test".into(),
            timestamp: 0,
        };
        let make_revision =
            |parent: Option<graft_core::ArtifactId>, path: &str, value: &[u8], change: u8| {
                let blob = Blob::from_bytes(value).expect("blob");
                store.insert(blob.artifact()).expect("blob");
                let tree = build_tree(
                    &store,
                    &std::collections::BTreeMap::from([(
                        path.to_owned(),
                        FileEntry {
                            kind: TreeEntryKind::File,
                            target: blob.id(),
                        },
                    )]),
                )
                .expect("tree");
                let revision = Revision::new(
                    vec![tree],
                    parent.into_iter().collect(),
                    ChangeId::from_bytes([change; 16]),
                    person.clone(),
                    person.clone(),
                    format!("change {change}"),
                )
                .expect("revision");
                store.insert(revision.artifact()).expect("revision")
            };
        let base = make_revision(None, "base.txt", b"base", 1);
        let target = make_revision(Some(base), "target.txt", b"target", 2);
        let source = make_revision(Some(base), "source.txt", b"source", 3);
        store.set_ref("branches/main", target).expect("target ref");
        store.set_ref("branches/dev", source).expect("source ref");

        let response = json_request(
            &app,
            Method::POST,
            "/api/repos/review/proposals",
            Some(&token),
            serde_json::json!({
                "source": "branches/dev",
                "target": "branches/main",
                "title": "Add source file"
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let proposal_id = body_json(response)
            .await
            .get("id")
            .and_then(Value::as_i64)
            .expect("proposal id");
        let response = json_request(
            &app,
            Method::POST,
            &format!("/api/repos/review/proposals/{proposal_id}/comments"),
            Some(&token),
            serde_json::json!({"body": "looks good"}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let response = json_request(
            &app,
            Method::POST,
            &format!("/api/repos/review/proposals/{proposal_id}/threads"),
            Some(&token),
            serde_json::json!({
                "revision": source.to_string(),
                "path": "source.txt",
                "line": 1,
                "body": "please keep this"
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let response = json_request(
            &app,
            Method::POST,
            &format!("/api/repos/review/proposals/{proposal_id}/approvals"),
            Some(&token),
            serde_json::json!({}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let response = request(
            &app,
            Method::GET,
            &format!("/api/repos/review/proposals/{proposal_id}"),
            Some(&token),
            Vec::new(),
            "application/json",
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let detail = body_json(response).await;
        assert_eq!(detail["changes"].as_array().expect("changes").len(), 1);
        assert_eq!(detail["threads"].as_array().expect("threads").len(), 1);

        let response = request(
            &app,
            Method::POST,
            &format!("/api/repos/review/proposals/{proposal_id}/land"),
            Some(&token),
            Vec::new(),
            "application/json",
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let landed = store.get_ref("branches/main").expect("main ref");
        assert_ne!(landed, Some(target));
        assert_eq!(
            store
                .get_ref(&format!("proposals/{proposal_id}"))
                .expect("proposal ref"),
            None
        );
        std::fs::remove_dir_all(config.data_dir).expect("test cleanup");
    }
}
