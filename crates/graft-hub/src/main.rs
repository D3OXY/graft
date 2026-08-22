use std::{collections::BTreeSet, fs, net::SocketAddr, path::PathBuf, sync::Arc};

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
use graft_core::{store::StoreError, RepoStore};
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

impl IntoResponse for ApiFailure {
    fn into_response(self) -> Response {
        let message = self.0.to_string();
        let status = match &self.0 {
            HubError::Unauthorized => StatusCode::UNAUTHORIZED,
            HubError::NotFound => StatusCode::NOT_FOUND,
            HubError::Forbidden => StatusCode::FORBIDDEN,
            HubError::Conflict => StatusCode::CONFLICT,
            HubError::InvalidRequest | HubError::Password => StatusCode::BAD_REQUEST,
            HubError::Database(_) | HubError::Io(_) | HubError::Sync(_) | HubError::Store(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
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
         CREATE TABLE IF NOT EXISTS profiles (user_id INTEGER PRIMARY KEY REFERENCES users(id), member_id TEXT NOT NULL, age_recipient TEXT NOT NULL, signing_public TEXT NOT NULL);",
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
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };

    use axum::{
        body::{to_bytes, Body},
        http::{header, Method, Request, StatusCode},
    };
    use graft_core::{Blob, RepoStore};
    use graft_sync::{decode_frames, encode_frames, Direction, Frame, Handshake, WireRefUpdate};
    use serde_json::Value;
    use tower::ServiceExt;

    use super::{initialize_database, router, AppState, HubConfig};

    fn test_config() -> HubConfig {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
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
}
