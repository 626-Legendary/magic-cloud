mod core;
mod error;
mod storage;
mod thumbnails;
use crate::{
    core::{CHUNK, Core, MAX_FILE, Mutation, Node, Upload, hash, id, now, token},
    error::{Error, Result},
};
use argon2::{
    Argon2, PasswordHasher, PasswordVerifier,
    password_hash::{PasswordHash, SaltString},
};
use axum::{
    Json, Router,
    body::Body,
    extract::{ConnectInfo, DefaultBodyLimit, Path, Query, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use futures_util::StreamExt;
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::SeekFrom,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
#[derive(Clone)]
struct App {
    core: Arc<Mutex<Core>>,
    password: Arc<String>,
    origin: Arc<String>,
    proxy: Arc<String>,
}
impl App {
    async fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Core) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let c = self.core.clone();
        tokio::task::spawn_blocking(move || {
            let mut c = c.lock().map_err(|_| Error::bad("state_unavailable"))?;
            c.guard()?;
            let r = f(&mut c);
            if r.as_ref()
                .is_err_and(|e| e.1 == "storage_io_error" || e.1 == "database_error")
            {
                c.broken = true
            }
            r
        })
        .await
        .map_err(|_| Error::bad("worker_failed"))?
    }
}
fn cookie(h: &HeaderMap) -> Option<String> {
    h.get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|s| s.trim().strip_prefix("mc_session=").map(str::to_owned))
}
async fn security(State(a): State<App>, req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    let write = ![Method::GET, Method::HEAD, Method::OPTIONS].contains(req.method());
    let origin = req
        .headers()
        .get("origin")
        .and_then(|h| h.to_str().ok())
        .map(str::to_owned);
    let session_id = cookie(req.headers());
    let csrf_value = req
        .headers()
        .get("x-csrf-token")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let secured = async {
        if write && origin.as_deref() != Some(a.origin.as_str()) {
            return Err(Error(StatusCode::FORBIDDEN, "origin_rejected", Value::Null));
        }
        if path.starts_with("/api/")
            && !path.starts_with("/api/public/")
            && path != "/api/auth/login"
        {
            let sid = session_id.ok_or(Error(
                StatusCode::UNAUTHORIZED,
                "authentication_required",
                Value::Null,
            ))?;
            let csrf = csrf_value;
            a.run(move |c| {
                let v: Option<String> =
                    c.db.query_row(
                        "SELECT csrf FROM sessions WHERE hash=?1 AND expires>?2 AND generation=?3",
                        params![hash(sid), now(), c.generation],
                        |r| r.get(0),
                    )
                    .optional()?;
                let expected = v.ok_or(Error(
                    StatusCode::UNAUTHORIZED,
                    "authentication_required",
                    Value::Null,
                ))?;
                if write && {
                    use subtle::ConstantTimeEq;
                    !bool::from(csrf.as_bytes().ct_eq(expected.as_bytes()))
                } {
                    return Err(Error(StatusCode::FORBIDDEN, "csrf_rejected", Value::Null));
                }
                Ok(())
            })
            .await?;
        }
        Ok(())
    }
    .await;
    let mut response = match secured {
        Ok(()) => next.run(req).await,
        Err(e) => e.into_response(),
    };
    if response.status().is_client_error()
        && !response
            .headers()
            .get("content-type")
            .is_some_and(|v| v.to_str().unwrap_or("").starts_with("application/json"))
    {
        response = Error(response.status(), "invalid_request", Value::Null).into_response();
    }
    let request_id = id();
    if response.status().is_server_error() {
        eprintln!("request {request_id} failed: {}", response.status());
    }
    let h = response.headers_mut();
    h.insert("x-request-id", HeaderValue::from_str(&request_id).unwrap());
    h.insert(
        "cache-control",
        HeaderValue::from_static("private, no-store"),
    );
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    h.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    h.insert("content-security-policy",HeaderValue::from_static("default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' blob: data:; media-src 'self' blob:; frame-src 'self' blob:; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'; connect-src 'self'; worker-src 'self' blob:"));
    response
}
#[derive(Deserialize)]
struct Login {
    password: String,
}
async fn login(
    State(a): State<App>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(v): Json<Login>,
) -> Result<Response> {
    let ip = if peer.ip().to_string() == *a.proxy {
        headers
            .get("cf-connecting-ip")
            .and_then(|v| v.to_str().ok())
            .and_then(|x| x.parse::<std::net::IpAddr>().ok())
            .map(|x| x.to_string())
            .unwrap_or_else(|| peer.ip().to_string())
    } else {
        peer.ip().to_string()
    };
    let rate_ip = ip.clone();
    a.run(move |c| {
        let n = now();
        c.attempts.retain(|_, v| {
            v.retain(|t| *t > n - 600);
            !v.is_empty()
        });
        let total: usize = c.attempts.values().map(Vec::len).sum();
        let q = c.attempts.entry(rate_ip.clone()).or_default();
        if q.len() >= 10 || total >= 50 {
            c.audit(false, &rate_ip, "rate_limited")?;
            return Err(Error(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                json!({"retry_after":600}),
            ));
        }
        q.push(n);
        Ok(())
    })
    .await?;
    let ph = a.password.clone();
    let ok = tokio::task::spawn_blocking(move || {
        PasswordHash::new(&ph).is_ok_and(|p| {
            Argon2::default()
                .verify_password(v.password.as_bytes(), &p)
                .is_ok()
        })
    })
    .await
    .unwrap_or(false);
    let sid = token();
    let csrf = token();
    let session = sid.clone();
    let csrf_out = csrf.clone();
    a.run(move |c| {
        c.audit(ok, &ip, if ok { "success" } else { "invalid_credentials" })?;
        if ok {
            c.db.execute(
                "INSERT INTO sessions VALUES(?1,?2,?3,?4)",
                params![hash(session), now() + 7 * 86400, c.generation, csrf],
            )?;
        }
        Ok(())
    })
    .await?;
    if !ok {
        return Err(Error(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            Value::Null,
        ));
    }
    let mut r = Json(json!({"csrf":csrf_out})).into_response();
    r.headers_mut().insert(
        "set-cookie",
        HeaderValue::from_str(&format!(
            "mc_session={sid}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=604800"
        ))
        .unwrap(),
    );
    Ok(r)
}
async fn session(State(a): State<App>, h: HeaderMap) -> Result<Json<Value>> {
    let sid = cookie(&h).unwrap_or_default();
    a.run(move |c| {
        let csrf: String = c.db.query_row(
            "SELECT csrf FROM sessions WHERE hash=?1",
            [hash(sid)],
            |r| r.get(0),
        )?;
        Ok(Json(json!({"csrf":csrf,"expires_in":604800})))
    })
    .await
}
async fn logout(State(a): State<App>, h: HeaderMap) -> Result<Response> {
    let sid = cookie(&h).unwrap_or_default();
    a.run(move |c| {
        c.db.execute("DELETE FROM sessions WHERE hash=?1", [hash(sid)])?;
        Ok(())
    })
    .await?;
    let mut r = Json(json!({"ok":true})).into_response();
    r.headers_mut().insert(
        "set-cookie",
        HeaderValue::from_static(
            "mc_session=; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=0",
        ),
    );
    Ok(r)
}
#[derive(Deserialize, Default)]
struct Browse {
    parent: Option<String>,
    q: Option<String>,
    sort: Option<String>,
    desc: Option<bool>,
    recent: Option<bool>,
}
async fn nodes(State(a): State<App>, Query(q): Query<Browse>) -> Result<Json<Value>> {
    a.run(move |c| {
        let p = c.folder(q.parent.as_deref().unwrap_or("root"))?;
        let all = c.nodes()?;
        let mut ns: Vec<_> = all
            .iter()
            .filter(|n| {
                !n.deleted
                    && n.id != "root"
                    && if let Some(ref search) = q.q {
                        n.name.to_lowercase().contains(&search.to_lowercase())
                    } else if q.recent.unwrap_or(false) {
                        n.kind == "file"
                    } else {
                        n.parent.as_deref() == Some(&p.id)
                    }
            })
            .cloned()
            .collect();
        ns.sort_by(|a, b| match q.sort.as_deref() {
            Some("size") => a.size.cmp(&b.size),
            Some("type") => a.kind.cmp(&b.kind).then(a.name.cmp(&b.name)),
            Some("mtime") => a.mtime.cmp(&b.mtime),
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });
        if q.desc.unwrap_or(q.recent.unwrap_or(false)) {
            ns.reverse()
        }
        if q.recent.unwrap_or(false) {
            ns.truncate(100)
        }
        let mut crumbs = vec![p.clone()];
        let mut cur = p;
        while let Some(ref pid) = cur.parent {
            cur = c.node(pid)?;
            crumbs.push(cur.clone())
        }
        crumbs.reverse();
        Ok(Json(json!({"nodes":ns,"breadcrumbs":crumbs})))
    })
    .await
}
#[derive(Deserialize)]
struct Create {
    parent: String,
    name: String,
}
async fn mkdir(State(a): State<App>, Json(v): Json<Create>) -> Result<Json<Node>> {
    a.run(move |c| c.mkdir(&v.parent, &v.name).map(Json)).await
}
#[derive(Deserialize)]
struct Relocate {
    parent: String,
    name: String,
    #[serde(default = "ask")]
    strategy: String,
}
fn ask() -> String {
    "ask".into()
}
async fn relocate(
    State(a): State<App>,
    Path(id): Path<String>,
    Json(v): Json<Relocate>,
) -> Result<Json<Node>> {
    a.run(move |c| c.relocate(&id, &v.parent, &v.name, &v.strategy).map(Json))
        .await
}
async fn detail(State(a): State<App>, Path(id): Path<String>) -> Result<Json<Node>> {
    a.run(move |c| {
        let n = c.node(&id)?;
        c.verify(&n)?;
        Ok(Json(n))
    })
    .await
}
async fn remove(State(a): State<App>, Path(id): Path<String>) -> Result<Json<Value>> {
    a.run(move |c| {
        let n = c.node(&id)?;
        let mut m = Mutation::default();
        c.trash_plan(&n, &mut m)?;
        c.commit(m)?;
        Ok(Json(json!({"ok":true})))
    })
    .await
}
async fn trash(State(a): State<App>) -> Result<Json<Vec<Value>>> {
    a.run(|c|Ok(Json(c.db.prepare("SELECT t.id,t.name,t.parent,t.deleted_at,n.size,n.kind FROM trash t JOIN nodes n ON n.id=t.id ORDER BY deleted_at DESC")?.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"parent":r.get::<_,String>(2)?,"deleted_at":r.get::<_,i64>(3)?,"size":r.get::<_,i64>(4)?,"kind":r.get::<_,String>(5)?})))?.collect::<std::result::Result<Vec<_>,_>>()?))).await
}
#[derive(Deserialize)]
struct Restore {
    parent: Option<String>,
    #[serde(default = "ask")]
    strategy: String,
}
async fn restore(
    State(a): State<App>,
    Path(id): Path<String>,
    Json(v): Json<Restore>,
) -> Result<Json<Node>> {
    a.run(move |c| c.restore(&id, v.parent.as_deref(), &v.strategy).map(Json))
        .await
}
async fn purge(State(a): State<App>, Path(id): Path<String>) -> Result<Json<Value>> {
    a.run(move |c| {
        c.purge(&id)?;
        Ok(Json(json!({"ok":true})))
    })
    .await
}
async fn empty(State(a): State<App>) -> Result<Json<Value>> {
    a.run(move |c| {
        let ids =
            c.db.prepare("SELECT id FROM trash")?
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
        for id in ids {
            c.purge(&id)?
        }
        Ok(Json(json!({"ok":true})))
    })
    .await
}
#[derive(Deserialize)]
struct UploadCreate {
    parent: String,
    name: String,
    size: u64,
    digest: String,
    #[serde(default = "ask")]
    strategy: String,
}
fn digest_ok(d: &str) -> bool {
    d.len() == 64
        && d.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
async fn create_upload(State(a): State<App>, Json(v): Json<UploadCreate>) -> Result<Json<Value>> {
    a.run(move|c|{if v.size>MAX_FILE||!digest_ok(&v.digest){return Err(Error::bad("invalid_upload"))}let p=c.folder(&v.parent)?;c.destination(&p,&v.name,&v.strategy,None)?;c.reserve(v.size)?;let id=id();let f=c.storage.file(&format!(".magic-cloud/uploads/{id}"),libc::O_RDWR|libc::O_CREAT|libc::O_EXCL)?;f.sync_all()?;c.db.execute("INSERT INTO uploads(id,parent,name,size,digest,strategy,expires) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![id,v.parent,v.name,v.size as i64,v.digest,v.strategy,now()+86400])?;Ok(Json(json!({"upload":c.upload(&id)?,"chunk_size":CHUNK,"chunks":[]})))}).await
}
async fn uploads(State(a): State<App>) -> Result<Json<Vec<Upload>>> {
    a.run(|c| {
        let ids =
            c.db.prepare("SELECT id FROM uploads WHERE state='pending' AND expires>?1")?
                .query_map([now()], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Json(
            ids.iter()
                .map(|id| c.upload(id))
                .collect::<Result<Vec<_>>>()?,
        ))
    })
    .await
}
async fn upload_status(State(a): State<App>, Path(id): Path<String>) -> Result<Json<Value>> {
    a.run(move |c| {
        Ok(Json(
            json!({"upload":c.upload(&id)?,"chunk_size":CHUNK,"chunks":c.chunks(&id)?}),
        ))
    })
    .await
}
struct ChunkGuard {
    core: Arc<Mutex<Core>>,
    key: (String, u64),
}
impl Drop for ChunkGuard {
    fn drop(&mut self) {
        if let Ok(mut c) = self.core.lock() {
            c.inflight.remove(&self.key);
        }
    }
}
async fn chunk(
    State(a): State<App>,
    Path((id, index)): Path<(String, u64)>,
    h: HeaderMap,
    body: Body,
) -> Result<Json<Value>> {
    let digest = h
        .get("x-chunk-sha256")
        .and_then(|x| x.to_str().ok())
        .unwrap_or("")
        .to_owned();
    if !digest_ok(&digest) {
        return Err(Error::bad("checksum_required"));
    }
    let uid = id.clone();
    let claimed_digest = digest.clone();
    let (file, len, duplicate) = a
        .run(move |c| {
            let u = c.upload(&uid)?;
            c.pending(&u)?;
            if index >= u.size.div_ceil(CHUNK) {
                return Err(Error::bad("invalid_chunk_index"));
            }
            let active: std::collections::HashSet<_> = c.inflight.iter().map(|(x, _)| x).collect();
            if c.inflight.contains(&(uid.clone(), index))
                || c.inflight.iter().filter(|(x, _)| x == &uid).count() >= 2
                || (!active.contains(&uid) && active.len() >= 2)
            {
                return Err(Error(
                    StatusCode::TOO_MANY_REQUESTS,
                    "upload_concurrency_limit",
                    Value::Null,
                ));
            }
            let prev: Option<String> =
                c.db.query_row(
                    "SELECT digest FROM chunks WHERE upload=?1 AND idx=?2",
                    params![uid, index as i64],
                    |r| r.get(0),
                )
                .optional()?;
            if prev.as_ref().is_some_and(|p| p != &claimed_digest) {
                return Err(Error::bad("chunk_conflict"));
            }
            let duplicate = prev.is_some();
            let f = c.storage.file(
                &format!(".magic-cloud/uploads/{uid}"),
                if duplicate {
                    libc::O_RDONLY
                } else {
                    libc::O_RDWR
                },
            )?;
            c.inflight.insert((uid, index));
            Ok((f, (u.size - index * CHUNK).min(CHUNK), duplicate))
        })
        .await?;
    let _guard = ChunkGuard {
        core: a.core.clone(),
        key: (id.clone(), index),
    };
    let mut file = tokio::fs::File::from_std(file);
    if !duplicate {
        file.seek(SeekFrom::Start(index * CHUNK)).await?;
    }
    let mut stream = body.into_data_stream();
    let mut received = 0u64;
    let mut sha = Sha256::new();
    while let Some(bytes) = tokio::time::timeout(std::time::Duration::from_secs(60), stream.next())
        .await
        .map_err(|_| Error::bad("chunk_timeout"))?
    {
        let bytes = bytes.map_err(|_| Error::bad("chunk_body_error"))?;
        received += bytes.len() as u64;
        if received > len {
            return Err(Error::bad("chunk_length_mismatch"));
        }
        sha.update(&bytes);
        if !duplicate {
            file.write_all(&bytes).await?;
        }
    }
    if received != len || hex::encode(sha.finalize()) != digest {
        return Err(Error::bad("chunk_checksum_mismatch"));
    }
    if !duplicate {
        file.sync_all().await?;
    }
    a.run(move |c| {
        c.pending(&c.upload(&id)?)?;
        if !duplicate {
            c.db.execute(
                "INSERT INTO chunks VALUES(?1,?2,?3,?4)",
                params![id, index as i64, digest, len as i64],
            )?;
        }
        Ok(Json(json!({"index":index,"size":len})))
    })
    .await
}
async fn complete(State(a): State<App>, Path(id): Path<String>) -> Result<Json<Node>> {
    a.run(move |c| c.complete(&id).map(Json)).await
}
#[derive(Deserialize)]
struct Strategy {
    strategy: String,
}
async fn upload_strategy(
    State(a): State<App>,
    Path(id): Path<String>,
    Json(v): Json<Strategy>,
) -> Result<Json<Value>> {
    a.run(move |c| {
        let u = c.upload(&id)?;
        c.pending(&u)?;
        if !["overwrite", "rename", "ask"].contains(&v.strategy.as_str()) {
            return Err(Error::bad("invalid_strategy"));
        }
        c.db.execute(
            "UPDATE uploads SET strategy=?2 WHERE id=?1",
            params![id, v.strategy],
        )?;
        Ok(Json(json!({"ok":true})))
    })
    .await
}
async fn cancel(State(a): State<App>, Path(id): Path<String>) -> Result<Json<Value>> {
    a.run(move |c| {
        let u = c.upload(&id)?;
        if u.state == "complete" {
            return Err(Error::bad("upload_already_complete"));
        }
        if c.inflight.iter().any(|(x, _)| x == &id) {
            return Err(Error(StatusCode::CONFLICT, "chunks_in_flight", Value::Null));
        }
        let p = format!(".magic-cloud/uploads/{id}");
        if c.storage.exists(&p) {
            c.storage.remove(&p)?
        }
        c.db.execute("UPDATE uploads SET state='cancelled' WHERE id=?1", [id])?;
        Ok(Json(json!({"ok":true})))
    })
    .await
}
#[derive(Deserialize)]
struct ShareCreate {
    root: String,
    expires: Option<i64>,
}
async fn create_share(State(a): State<App>, Json(v): Json<ShareCreate>) -> Result<Json<Value>> {
    a.run(move |c| {
        let n = c.node(&v.root)?;
        c.verify(&n)?;
        if v.expires.is_some_and(|t| t <= now()) {
            return Err(Error::bad("invalid_expiry"));
        }
        let t = token();
        let sid = id();
        c.db.execute(
            "INSERT INTO shares VALUES(?1,?2,?3,?4,0,?5,?6)",
            params![sid, hash(&t), v.root, v.expires, now(), c.seal_token(&t)?],
        )?;
        Ok(Json(
            json!({"id":sid,"token":t,"url":format!("/s/{t}"),"expires":v.expires}),
        ))
    })
    .await
}
async fn shares(State(a): State<App>) -> Result<Json<Vec<Value>>> {
    a.run(|c|Ok(Json(c.db.prepare("SELECT s.id,s.root,n.name,s.expires,s.revoked,s.created,s.encrypted_token FROM shares s JOIN nodes n ON s.root=n.id ORDER BY s.created DESC")?.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"root":r.get::<_,String>(1)?,"name":r.get::<_,String>(2)?,"expires":r.get::<_,Option<i64>>(3)?,"revoked":r.get::<_,bool>(4)?,"created":r.get::<_,i64>(5)?,"url":c.unseal_token(&r.get::<_,String>(6)?).ok().map(|t|format!("/s/{t}"))})))?.collect::<std::result::Result<Vec<_>,_>>()?))).await
}
async fn revoke(State(a): State<App>, Path(id): Path<String>) -> Result<Json<Value>> {
    a.run(move |c| {
        c.db.execute("UPDATE shares SET revoked=1 WHERE id=?1", [id])?;
        Ok(Json(json!({"ok":true})))
    })
    .await
}
#[derive(Deserialize)]
struct Expiry {
    expires: Option<i64>,
}
async fn expiry(
    State(a): State<App>,
    Path(id): Path<String>,
    Json(v): Json<Expiry>,
) -> Result<Json<Value>> {
    a.run(move |c| {
        if v.expires.is_some_and(|t| t <= now()) {
            return Err(Error::bad("invalid_expiry"));
        }
        c.db.execute(
            "UPDATE shares SET expires=?2 WHERE id=?1",
            params![id, v.expires],
        )?;
        Ok(Json(json!({"ok":true})))
    })
    .await
}
fn share_root(c: &Core, t: &str) -> Result<Node> {
    let root:Option<String>=c.db.query_row("SELECT root FROM shares WHERE hash=?1 AND revoked=0 AND (expires IS NULL OR expires>?2)",params![hash(t),now()],|r|r.get(0)).optional()?;
    let n = c.node(&root.ok_or_else(Error::missing)?)?;
    c.verify(&n)?;
    Ok(n)
}
#[derive(Deserialize)]
struct PublicQuery {
    node: Option<String>,
}
async fn public(
    State(a): State<App>,
    Path(t): Path<String>,
    Query(q): Query<PublicQuery>,
) -> Result<Json<Value>> {
    a.run(move |c| {
        let root = share_root(c, &t)?;
        let n = if let Some(id) = q.node {
            if !c.within(&id, &root.id)? {
                return Err(Error::missing());
            }
            c.node(&id)?
        } else {
            root.clone()
        };
        c.verify(&n)?;
        let ns: Vec<_> = c
            .nodes()?
            .into_iter()
            .filter(|x| !x.deleted && x.parent.as_deref() == Some(&n.id))
            .map(public_node)
            .collect();
        Ok(Json(
            json!({"root":public_node(root),"node":public_node(n),"nodes":ns}),
        ))
    })
    .await
}
fn public_node(n: Node) -> Value {
    json!({"id":n.id,"name":n.name,"kind":n.kind,"size":n.size,"mtime":n.mtime})
}
#[derive(Deserialize, Default)]
struct Download {
    inline: Option<bool>,
}
async fn send_file(
    file: std::fs::File,
    name: String,
    h: HeaderMap,
    inline: bool,
) -> Result<Response> {
    let size = file.metadata()?.len();
    let mime = mime_guess::from_path(&name).first_or_octet_stream();
    let safe_inline = inline
        && (mime.type_() == "image"
            || mime.type_() == "audio"
            || mime.essence_str() == "application/pdf");
    let mut start = 0;
    let mut end = size.saturating_sub(1);
    let mut partial = false;
    if let Some(range) = h.get("range").and_then(|x| x.to_str().ok()) {
        let invalid = || {
            Error(
                StatusCode::RANGE_NOT_SATISFIABLE,
                "invalid_range",
                json!({"size":size}),
            )
        };
        let r = range.strip_prefix("bytes=").ok_or_else(invalid)?;
        let (s, e) = r.split_once('-').ok_or_else(invalid)?;
        if s.is_empty() {
            let suffix = e.parse::<u64>().map_err(|_| invalid())?;
            if suffix == 0 {
                return Err(invalid());
            }
            start = size.saturating_sub(suffix);
        } else {
            start = s.parse().map_err(|_| invalid())?;
            if !e.is_empty() {
                end = e.parse::<u64>().map_err(|_| invalid())?.min(end)
            }
        }
        if start > end || size == 0 {
            return Err(invalid());
        }
        partial = true;
    }
    let len = if size == 0 { 0 } else { end - start + 1 };
    let mut f = tokio::fs::File::from_std(file);
    f.seek(SeekFrom::Start(start)).await?;
    let mut r = Response::new(Body::from_stream(tokio_util::io::ReaderStream::new(
        f.take(len),
    )));
    *r.status_mut() = if partial {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };
    let headers = r.headers_mut();
    headers.insert(
        "content-type",
        HeaderValue::from_str(if safe_inline {
            mime.as_ref()
        } else {
            "application/octet-stream"
        })
        .unwrap(),
    );
    headers.insert(
        "content-length",
        HeaderValue::from_str(&len.to_string()).unwrap(),
    );
    headers.insert("accept-ranges", HeaderValue::from_static("bytes"));
    headers.insert(
        "content-disposition",
        HeaderValue::from_str(&format!(
            "{}; filename*=UTF-8''{}",
            if safe_inline { "inline" } else { "attachment" },
            percent_encoding::utf8_percent_encode(&name, percent_encoding::NON_ALPHANUMERIC)
        ))
        .unwrap(),
    );
    if partial {
        headers.insert(
            "content-range",
            HeaderValue::from_str(&format!("bytes {start}-{end}/{size}")).unwrap(),
        );
    }
    if safe_inline && mime.essence_str() == "application/pdf" {
        headers.insert(
            "content-security-policy",
            HeaderValue::from_static("sandbox; default-src 'none'"),
        );
    }
    Ok(r)
}
async fn download(
    State(a): State<App>,
    Path(id): Path<String>,
    Query(q): Query<Download>,
    h: HeaderMap,
) -> Result<Response> {
    let (f, n) = a
        .run(move |c| {
            let n = c.node(&id)?;
            c.verify(&n)?;
            if n.kind != "file" {
                return Err(Error::bad("file_required"));
            }
            Ok((c.open_node(&n)?, n.name))
        })
        .await?;
    send_file(f, n, h, q.inline.unwrap_or(false)).await
}
async fn public_download(
    State(a): State<App>,
    Path((t, id)): Path<(String, String)>,
    h: HeaderMap,
) -> Result<Response> {
    let (f, n) = a
        .run(move |c| {
            let root = share_root(c, &t)?;
            if !c.within(&id, &root.id)? {
                return Err(Error::missing());
            }
            let n = c.node(&id)?;
            c.verify(&n)?;
            if n.kind != "file" {
                return Err(Error::bad("file_required"));
            }
            Ok((c.open_node(&n)?, n.name))
        })
        .await?;
    send_file(f, n, h, false).await
}
async fn text_preview(State(a): State<App>, Path(id): Path<String>) -> Result<Json<Value>> {
    a.run(move |c| {
        use std::io::Read;
        let n = c.node(&id)?;
        c.verify(&n)?;
        if n.kind != "file" || n.size > 2 * 1024 * 1024 {
            return Err(Error::bad("text_preview_limit"));
        }
        let mut text = String::new();
        c.storage
            .file(&n.path, libc::O_RDONLY)?
            .read_to_string(&mut text)
            .map_err(|_| Error::bad("unsupported_text_encoding"))?;
        Ok(Json(json!({"text":text})))
    })
    .await
}
async fn settings(State(a): State<App>) -> Result<Json<Value>> {
    a.run(|c|{let(total,free)=c.storage.space()?;Ok(Json(json!({"storage":{"online":true,"total":total,"free":free,"used":total-free,"safety_margin":c.reserve_bytes.max(total*c.reserve_percent/100)},"system":{"version":env!("CARGO_PKG_VERSION"),"architecture":std::env::consts::ARCH,"os":std::env::consts::OS},"limits":{"max_file_size":MAX_FILE,"chunk_size":CHUNK,"upload_files":2,"chunks_per_file":2}})))}).await
}
async fn audit(State(a): State<App>) -> Result<Json<Vec<Value>>> {
    a.run(|c|Ok(Json(c.db.prepare("SELECT time,success,ip,category FROM login_audit ORDER BY id DESC LIMIT 1000")?.query_map([],|r|Ok(json!({"time":r.get::<_,i64>(0)?,"success":r.get::<_,bool>(1)?,"ip":r.get::<_,String>(2)?,"category":r.get::<_,String>(3)?})))?.collect::<std::result::Result<Vec<_>,_>>()?))).await
}
async fn backups(State(a): State<App>) -> Result<Json<Vec<String>>> {
    a.run(|c| {
        let mut ns = std::fs::read_dir(format!("{}/.magic-cloud/backups", c.storage.path))?
            .filter_map(|e| e.ok().and_then(|e| e.file_name().into_string().ok()))
            .filter(|n| n.ends_with(".db"))
            .collect::<Vec<_>>();
        ns.sort();
        ns.reverse();
        Ok(Json(ns))
    })
    .await
}
async fn backup(State(a): State<App>) -> Result<Json<Value>> {
    a.run(|c| Ok(Json(json!({"name":c.backup()?})))).await
}
async fn backup_download(
    State(a): State<App>,
    Path(name): Path<String>,
    h: HeaderMap,
) -> Result<Response> {
    let out = name.clone();
    let f = a
        .run(move |c| {
            storage::name(&name)?;
            if !name.ends_with(".db") {
                return Err(Error::missing());
            }
            c.storage
                .file(&format!(".magic-cloud/backups/{name}"), libc::O_RDONLY)
        })
        .await?;
    send_file(f, out, h, false).await
}
fn routes(a: App, static_dir: &str) -> Router {
    Router::new()
        .route("/health/live", get(|| async { "ok" }))
        .route(
            "/health/ready",
            get({
                let a = a.clone();
                move || {
                    let a = a.clone();
                    async move { a.run(|_| Ok("ok")).await }
                }
            }),
        )
        .route("/api/auth/login", post(login))
        .route("/api/auth/session", get(session))
        .route("/api/auth/logout", post(logout))
        .route("/api/nodes", get(nodes).post(mkdir))
        .route(
            "/api/nodes/{id}",
            get(detail).patch(relocate).delete(remove),
        )
        .route("/api/nodes/{id}/download", get(download))
        .route("/api/nodes/{id}/text", get(text_preview))
        .route("/api/nodes/{id}/thumbnail", get(thumbnails::thumbnail))
        .route("/api/trash", get(trash).delete(empty))
        .route("/api/trash/{id}", delete(purge))
        .route("/api/trash/{id}/restore", post(restore))
        .route("/api/uploads", get(uploads).post(create_upload))
        .route("/api/uploads/{id}", get(upload_status).delete(cancel))
        .route("/api/uploads/{id}/chunks/{index}", put(chunk))
        .route("/api/uploads/{id}/complete", post(complete))
        .route(
            "/api/uploads/{id}/strategy",
            axum::routing::patch(upload_strategy),
        )
        .route("/api/shares", get(shares).post(create_share))
        .route("/api/shares/{id}", delete(revoke).patch(expiry))
        .route("/api/public/{token}", get(public))
        .route(
            "/api/public/{token}/nodes/{id}/download",
            get(public_download),
        )
        .route("/api/settings", get(settings))
        .route("/api/settings/audit", get(audit))
        .route("/api/settings/backups", get(backups).post(backup))
        .route("/api/settings/backups/{name}", get(backup_download))
        .route("/api/{*rest}", get(|| async { Error::missing() }))
        .fallback_service(tower_http::services::ServeDir::new(static_dir).fallback(
            tower_http::services::ServeFile::new(format!("{static_dir}/index.html")),
        ))
        .layer(DefaultBodyLimit::max(CHUNK as usize))
        .layer(tower_http::limit::RequestBodyLimitLayer::new(
            CHUNK as usize,
        ))
        .layer(middleware::from_fn_with_state(a.clone(), security))
        .with_state(a)
}
#[tokio::main]
async fn main() {
    if let Err(e) = start().await {
        eprintln!("startup failed: {}", e.1);
        std::process::exit(1)
    }
}
async fn start() -> Result<()> {
    let env = |n: &str| std::env::var(n).map_err(|_| Error::bad("missing_configuration"));
    if std::env::args().any(|a| a == "--diagnose") {
        let storage = storage::Storage::open(&env("STORAGE_ROOT")?, &env("STORAGE_ID")?, true)?;
        let report = core::diagnose(&storage)?;
        println!("{}", report);
        return Ok(());
    }
    let password = env("APP_PASSWORD")?;
    let secret = env("SESSION_SECRET")?;
    if password.len() < 12 || secret.len() < 32 {
        return Err(Error::bad("weak_configuration"));
    }
    let origin = env("APP_ORIGIN")?;
    if !origin.starts_with("https://") || origin.ends_with('/') {
        return Err(Error::bad("https_origin_required"));
    }
    let storage = storage::Storage::open(&env("STORAGE_ROOT")?, &env("STORAGE_ID")?, true)?;
    let mut c = Core::open(storage, &secret, &password)?;
    c.reserve_bytes = std::env::var("SAFETY_MARGIN_BYTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2 * 1024 * 1024 * 1024);
    c.reserve_percent = std::env::var("SAFETY_MARGIN_PERCENT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    let salt = SaltString::generate(&mut rand::rngs::OsRng);
    let ph = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|_| Error::bad("password_hash_error"))?
        .to_string();
    let a = App {
        core: Arc::new(Mutex::new(c)),
        password: Arc::new(ph),
        origin: Arc::new(origin),
        proxy: Arc::new(env("TRUSTED_PROXY_IP")?),
    };
    let thumbnail_jobs = a.clone();
    tokio::spawn(async move {
        loop {
            if let Ok(ids) = thumbnail_jobs
                .run(|c| {
                    Ok(c.nodes()?
                        .into_iter()
                        .filter(|n| {
                            !n.deleted && n.kind == "file" && thumbnails::supported(&n.name)
                        })
                        .map(|n| n.id)
                        .collect::<Vec<_>>())
                })
                .await
            {
                for id in ids {
                    let _ = thumbnails::thumbnail(State(thumbnail_jobs.clone()), Path(id)).await;
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });
    let watchdog = a.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
        loop {
            tick.tick().await;
            if watchdog.run(|_| Ok(())).await.is_err() {
                eprintln!("storage unavailable; stopping backend");
                std::process::exit(74)
            }
        }
    });
    let backup_interval = std::env::var("BACKUP_INTERVAL_SECONDS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|n| *n >= 3600)
        .unwrap_or(86400);
    let backup_keep = std::env::var("BACKUP_KEEP")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n >= 1)
        .unwrap_or(7);
    let jobs = a.clone();
    tokio::spawn(async move {
        let mut last = 0;
        loop {
            let backup_due = now() - last >= backup_interval;
            let r = jobs
                .run(move |c| {
                    c.maintain()?;
                    if backup_due {
                        c.backup()?;
                        let mut files =
                            std::fs::read_dir(format!("{}/.magic-cloud/backups", c.storage.path))?
                                .filter_map(|x| {
                                    x.ok().and_then(|e| e.file_name().into_string().ok())
                                })
                                .filter(|s| s.ends_with(".db"))
                                .collect::<Vec<_>>();
                        files.sort();
                        let remove = files.len().saturating_sub(backup_keep);
                        for name in files.into_iter().take(remove) {
                            c.storage.remove(&format!(".magic-cloud/backups/{name}"))?;
                        }
                    }
                    Ok(())
                })
                .await;
            if r.is_ok() && backup_due {
                last = now()
            }
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });
    let port = std::env::var("PORT").unwrap_or("5080".into());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    eprintln!("Magic Cloud ready");
    axum::serve(
        listener,
        routes(
            a,
            &std::env::var("STATIC_DIR").unwrap_or("/app/public".into()),
        )
        .into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}
#[cfg(test)]
mod tests;
