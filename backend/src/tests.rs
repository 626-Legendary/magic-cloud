use super::*;
use http_body_util::BodyExt;
use std::{
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, symlink},
};
use tower::ServiceExt;
struct Fixture {
    dir: tempfile::TempDir,
    app: App,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("files")).unwrap();
        for d in [
            "database",
            "uploads",
            "trash",
            "operations",
            "thumbnails",
            "backups",
        ] {
            fs::create_dir_all(dir.path().join(".magic-cloud").join(d)).unwrap()
        }
        fs::write(dir.path().join(".magic-cloud/storage-id"), "test-disk").unwrap();
        let storage =
            storage::Storage::open(dir.path().to_str().unwrap(), "test-disk", false).unwrap();
        let mut core = Core::open(
            storage,
            "test-session-secret-at-least-32-characters",
            "correct-test-password",
        )
        .unwrap();
        core.reserve_bytes = 0;
        core.reserve_percent = 0;
        let ph = Argon2::default()
            .hash_password(
                b"correct-test-password",
                &SaltString::generate(&mut rand::rngs::OsRng),
            )
            .unwrap()
            .to_string();
        Self {
            dir,
            app: App {
                core: Arc::new(Mutex::new(core)),
                password: Arc::new(ph),
                origin: Arc::new("https://cloud.test".into()),
                proxy: Arc::new("172.30.56.3".into()),
            },
        }
    }
    fn core(&self) -> std::sync::MutexGuard<'_, Core> {
        self.app.core.lock().unwrap()
    }
    async fn request(
        &self,
        method: &str,
        path: &str,
        data: Value,
        cookie: Option<&str>,
        csrf: Option<&str>,
    ) -> (StatusCode, HeaderMap, Value) {
        let mut req = axum::http::Request::builder()
            .method(method)
            .uri(path)
            .header("origin", "https://cloud.test")
            .header("content-type", "application/json");
        if let Some(c) = cookie {
            req = req.header("cookie", c)
        }
        if let Some(t) = csrf {
            req = req.header("x-csrf-token", t)
        }
        let mut req = req.body(Body::from(data.to_string())).unwrap();
        req.extensions_mut()
            .insert(ConnectInfo("127.0.0.1:4321".parse::<SocketAddr>().unwrap()));
        let res = routes(self.app.clone(), "/no-static")
            .oneshot(req)
            .await
            .unwrap();
        let status = res.status();
        let h = res.headers().clone();
        let body = res.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            h,
            serde_json::from_slice(&body).unwrap_or(Value::Null),
        )
    }
    async fn auth(&self) -> (String, String) {
        let (s, h, v) = self
            .request(
                "POST",
                "/api/auth/login",
                json!({"password":"correct-test-password"}),
                None,
                None,
            )
            .await;
        assert_eq!(s, StatusCode::OK);
        (
            h["set-cookie"]
                .to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .into(),
            v["csrf"].as_str().unwrap().into(),
        )
    }
}
fn add_file(c: &mut Core, parent: &str, name: &str, data: &[u8]) -> Node {
    let p = c.folder(parent).unwrap();
    let path = format!("{}/{name}", p.path);
    let mut f = c
        .storage
        .file(&path, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL)
        .unwrap();
    f.write_all(data).unwrap();
    f.sync_all().unwrap();
    let m = f.metadata().unwrap();
    let n = Node {
        id: id(),
        parent: Some(parent.into()),
        name: name.into(),
        path,
        kind: "file".into(),
        size: data.len() as u64,
        mtime: m.mtime(),
        mtime_ns: m.mtime_nsec(),
        dev: m.dev(),
        ino: m.ino(),
        deleted: false,
    };
    c.put_node(&n).unwrap();
    n
}
#[test]
fn lifecycle_conflicts_shares_and_restore() {
    let f = Fixture::new();
    let mut c = f.core();
    let a = c.mkdir("root", "A").unwrap();
    let b = c.mkdir("root", "B").unwrap();
    let n = add_file(&mut c, &a.id, "notes.txt", b"first");
    let t = token();
    c.db.execute(
        "INSERT INTO shares VALUES(?1,?2,?3,NULL,0,?4,?5)",
        params![id(), hash(&t), n.id, now(), c.seal_token(&t).unwrap()],
    )
    .unwrap();
    let moved = c.relocate(&n.id, &b.id, "renamed.txt", "ask").unwrap();
    assert_eq!(moved.id, n.id);
    assert_eq!(share_root(&c, &t).unwrap().id, n.id);
    assert!(!c.within(&n.id, &a.id).unwrap());
    let old = add_file(&mut c, &b.id, "target.txt", b"old");
    assert_eq!(
        c.relocate(&n.id, &b.id, "target.txt", "ask").unwrap_err().1,
        "name_conflict"
    );
    c.relocate(&n.id, &b.id, "target.txt", "overwrite").unwrap();
    assert!(c.node(&old.id).unwrap().deleted);
    assert_eq!(
        fs::read(f.dir.path().join(format!(".magic-cloud/trash/{}", old.id))).unwrap(),
        b"old"
    );
    c.restore(&old.id, None, "rename").unwrap();
    assert_eq!(c.node(&old.id).unwrap().name, "target (1).txt");
    let mut op = Mutation::default();
    c.trash_plan(&c.node(&n.id).unwrap(), &mut op).unwrap();
    c.commit(op).unwrap();
    assert!(share_root(&c, &t).is_err());
    c.restore(&n.id, None, "rename").unwrap();
    assert!(share_root(&c, &t).is_err());
    assert!(c.relocate(&a.id, &a.id, "A", "ask").is_err());
}
#[test]
fn traversal_symlink_replacement_and_storage_identity() {
    let f = Fixture::new();
    let mut c = f.core();
    let n = add_file(&mut c, "root", "safe.txt", b"safe");
    symlink("/etc/passwd", f.dir.path().join("files/link")).unwrap();
    assert!(c.storage.file("files/link", libc::O_RDONLY).is_err());
    assert!(c.storage.file("../etc/passwd", libc::O_RDONLY).is_err());
    fs::rename(
        f.dir.path().join("files/safe.txt"),
        f.dir.path().join("files/old.txt"),
    )
    .unwrap();
    fs::write(f.dir.path().join("files/safe.txt"), b"evil").unwrap();
    assert!(c.verify(&n).is_err());
    fs::write(f.dir.path().join(".magic-cloud/storage-id"), "different").unwrap();
    assert!(c.guard().is_err());
}
#[test]
fn crash_recovery_after_every_rename() {
    for stage in 0..=2 {
        let f = Fixture::new();
        let mut c = f.core();
        let old = add_file(&mut c, "root", "target", b"old");
        let mut new = add_file(&mut c, "root", "incoming", b"new");
        let mut op = Mutation::default();
        c.trash_plan(&old, &mut op).unwrap();
        op.moves.push(c.move_item(&new, old.path.clone()));
        new.path = old.path.clone();
        new.name = old.name.clone();
        op.nodes.push(new.clone());
        c.db.execute(
            "INSERT INTO operations VALUES('test',?1)",
            [serde_json::to_string(&op).unwrap()],
        )
        .unwrap();
        for mv in op.moves.iter().take(stage) {
            c.storage.rename(&mv.src, &mv.dst).unwrap()
        }
        c.recover().unwrap();
        c.recover().unwrap();
        assert_eq!(fs::read(f.dir.path().join("files/target")).unwrap(), b"new");
        assert!(c.node(&old.id).unwrap().deleted);
        assert_eq!(c.node(&new.id).unwrap().name, "target");
    }
}
#[test]
fn folder_restore_does_not_capture_replacement_or_separate_trash() {
    let f = Fixture::new();
    let mut c = f.core();
    let a = c.mkdir("root", "A").unwrap();
    let child = add_file(&mut c, &a.id, "child", b"a");
    let separately = add_file(&mut c, &a.id, "other", b"b");
    let mut op = Mutation::default();
    c.trash_plan(&separately, &mut op).unwrap();
    c.commit(op).unwrap();
    let mut op = Mutation::default();
    c.trash_plan(&a, &mut op).unwrap();
    c.commit(op).unwrap();
    let replacement = c.mkdir("root", "A").unwrap();
    let other = add_file(&mut c, &replacement.id, "child", b"replacement");
    c.restore(&a.id, None, "rename").unwrap();
    assert!(c.node(&separately.id).unwrap().deleted);
    assert_eq!(c.node(&other.id).unwrap().path, "files/A/child");
    assert_eq!(c.node(&child.id).unwrap().path, "files/A (1)/child");
}
#[test]
fn online_backup_and_missing_database_fail_closed() {
    let f = Fixture::new();
    {
        let mut c = f.core();
        c.mkdir("root", "persistent").unwrap();
        let name = c.backup().unwrap();
        let db = rusqlite::Connection::open(f.dir.path().join(".magic-cloud/backups").join(name))
            .unwrap();
        assert_eq!(
            db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM nodes", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
    }
    fs::rename(
        f.dir.path().join(".magic-cloud/database/magic-cloud.db"),
        f.dir.path().join(".magic-cloud/database/lost.db"),
    )
    .unwrap();
    let s = storage::Storage::open(f.dir.path().to_str().unwrap(), "test-disk", false).unwrap();
    assert!(Core::open(s, "secret", "password").is_err());
}
#[tokio::test]
async fn auth_csrf_expiry_logout_and_audit() {
    let f = Fixture::new();
    assert_eq!(
        f.request("GET", "/api/nodes", json!({}), None, None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.request(
            "POST",
            "/api/auth/login",
            json!({"password":"wrong"}),
            None,
            None
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (cookie, csrf) = f.auth().await;
    let (_, h, _) = f
        .request(
            "POST",
            "/api/auth/login",
            json!({"password":"correct-test-password"}),
            None,
            None,
        )
        .await;
    let attr = h["set-cookie"].to_str().unwrap();
    for flag in ["HttpOnly", "Secure", "SameSite=Strict", "Max-Age=604800"] {
        assert!(attr.contains(flag))
    }
    assert_eq!(
        f.request(
            "POST",
            "/api/nodes",
            json!({"parent":"root","name":"test"}),
            Some(&cookie),
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        f.request(
            "POST",
            "/api/nodes",
            json!({"parent":"root","name":"test"}),
            Some(&cookie),
            Some(&csrf)
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        f.request(
            "POST",
            "/api/auth/logout",
            json!({}),
            Some(&cookie),
            Some(&csrf)
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        f.request("GET", "/api/nodes", json!({}), Some(&cookie), None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let (cookie, _) = f.auth().await;
    f.core()
        .db
        .execute("UPDATE sessions SET expires=?1", [now() - 1])
        .unwrap();
    assert_eq!(
        f.request("GET", "/api/nodes", json!({}), Some(&cookie), None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    {
        let c = f.core();
        for _ in 0..1010 {
            c.db.execute(
                "INSERT INTO login_audit(time,success,ip) VALUES(0,0,'test')",
                [],
            )
            .unwrap();
        }
    }
    let _ = f.auth().await;
    assert_eq!(
        f.core()
            .db
            .query_row("SELECT COUNT(*) FROM login_audit", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1000
    );
}
#[tokio::test]
async fn rate_limit_and_origin() {
    let f = Fixture::new();
    for _ in 0..10 {
        assert_eq!(
            f.request(
                "POST",
                "/api/auth/login",
                json!({"password":"wrong"}),
                None,
                None
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        )
    }
    assert_eq!(
        f.request(
            "POST",
            "/api/auth/login",
            json!({"password":"wrong"}),
            None,
            None
        )
        .await
        .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("origin", "https://evil.test")
        .body(Body::from("{}"))
        .unwrap();
    assert_eq!(
        routes(f.app.clone(), "/no-static")
            .oneshot(req)
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
}
#[tokio::test]
async fn chunk_protocol_limits_idempotency_and_completion() {
    let f = Fixture::new();
    let (cookie, csrf) = f.auth().await;
    let data = b"upload contents";
    let digest = hash(data);
    let (s, _, u) = f
        .request(
            "POST",
            "/api/uploads",
            json!({"parent":"root","name":"file.txt","size":data.len(),"digest":digest}),
            Some(&cookie),
            Some(&csrf),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let uid = u["upload"]["id"].as_str().unwrap();
    assert!(!f.dir.path().join("files/file.txt").exists());
    for _ in 0..2 {
        let req = axum::http::Request::builder()
            .method("PUT")
            .uri(format!("/api/uploads/{uid}/chunks/0"))
            .header("origin", "https://cloud.test")
            .header("cookie", &cookie)
            .header("x-csrf-token", &csrf)
            .header("x-chunk-sha256", &digest)
            .body(Body::from(data.to_vec()))
            .unwrap();
        let res = routes(f.app.clone(), "/no-static")
            .oneshot(req)
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK)
    }
    let (s, _, n) = f
        .request(
            "POST",
            &format!("/api/uploads/{uid}/complete"),
            json!({}),
            Some(&cookie),
            Some(&csrf),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(fs::read(f.dir.path().join("files/file.txt")).unwrap(), data);
    let (s, _, again) = f
        .request(
            "POST",
            &format!("/api/uploads/{uid}/complete"),
            json!({}),
            Some(&cookie),
            Some(&csrf),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(n["id"], again["id"]);
    assert_eq!(
        f.request(
            "POST",
            "/api/uploads",
            json!({"parent":"root","name":"huge","size":MAX_FILE+1,"digest":digest}),
            Some(&cookie),
            Some(&csrf)
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    f.core().reserve_bytes = u64::MAX / 2;
    assert_eq!(
        f.request(
            "POST",
            "/api/uploads",
            json!({"parent":"root","name":"full","size":100,"digest":digest}),
            Some(&cookie),
            Some(&csrf)
        )
        .await
        .0,
        StatusCode::INSUFFICIENT_STORAGE
    );
}
#[tokio::test]
async fn share_subtree_dynamic_members_expiry_and_revocation() {
    let f = Fixture::new();
    let (cookie, csrf) = f.auth().await;
    let folder = f.core().mkdir("root", "shared").unwrap();
    let (_, _, v) = f
        .request(
            "POST",
            "/api/shares",
            json!({"root":folder.id}),
            Some(&cookie),
            Some(&csrf),
        )
        .await;
    let t = v["token"].as_str().unwrap();
    let child = add_file(&mut f.core(), &folder.id, "new.txt", b"contents");
    let (s, _, v) = f
        .request("GET", &format!("/api/public/{t}"), json!({}), None, None)
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["nodes"][0]["id"], child.id);
    assert!(v["nodes"][0].get("path").is_none());
    f.core()
        .relocate(&child.id, "root", "moved", "ask")
        .unwrap();
    assert_eq!(
        f.request(
            "GET",
            &format!("/api/public/{t}?node={}", child.id),
            json!({}),
            None,
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    f.core()
        .db
        .execute("UPDATE shares SET expires=?1", [now() - 1])
        .unwrap();
    assert_eq!(
        f.request("GET", &format!("/api/public/{t}"), json!({}), None, None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}
#[test]
fn retention_generation_and_token_encryption() {
    let f = Fixture::new();
    let mut c = f.core();
    let encrypted = c.seal_token("example").unwrap();
    assert!(!encrypted.contains("example"));
    assert_eq!(c.unseal_token(&encrypted).unwrap(), "example");
    let n = add_file(&mut c, "root", "old", b"data");
    let mut op = Mutation::default();
    c.trash_plan(&n, &mut op).unwrap();
    c.commit(op).unwrap();
    c.db.execute("UPDATE trash SET deleted_at=?1", [now() - 31 * 86400])
        .unwrap();
    c.maintain().unwrap();
    assert!(
        !f.dir
            .path()
            .join(format!(".magic-cloud/trash/{}", n.id))
            .exists()
    );
    assert!(c.restore(&n.id, None, "ask").is_err());
}

#[tokio::test]
async fn streaming_range_and_private_download_auth() {
    let f = Fixture::new();
    let n = add_file(&mut f.core(), "root", "sample.txt", b"0123456789");
    let (cookie, _) = f.auth().await;
    let req = axum::http::Request::builder()
        .uri(format!("/api/nodes/{}/download", n.id))
        .header("cookie", cookie)
        .header("range", "bytes=2-5")
        .body(Body::empty())
        .unwrap();
    let res = routes(f.app.clone(), "/none").oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(res.headers()["content-range"], "bytes 2-5/10");
    assert_eq!(res.headers()["cache-control"], "private, no-store");
    assert_eq!(
        res.into_body().collect().await.unwrap().to_bytes().as_ref(),
        b"2345"
    );
    assert_eq!(
        f.request(
            "GET",
            &format!("/api/nodes/{}/download", n.id),
            json!({}),
            None,
            None
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
}
#[tokio::test]
async fn upload_checksum_errors_concurrency_expiry_and_zero_length() {
    let f = Fixture::new();
    let (cookie, csrf) = f.auth().await;
    let data = b"expected";
    let (_, _, v) = f
        .request(
            "POST",
            "/api/uploads",
            json!({"parent":"root","name":"a","size":data.len(),"digest":hash(data)}),
            Some(&cookie),
            Some(&csrf),
        )
        .await;
    let uid = v["upload"]["id"].as_str().unwrap();
    let request = |checksum: &str, bytes: &'static [u8]| {
        axum::http::Request::builder()
            .method("PUT")
            .uri(format!("/api/uploads/{uid}/chunks/0"))
            .header("origin", "https://cloud.test")
            .header("cookie", &cookie)
            .header("x-csrf-token", &csrf)
            .header("x-chunk-sha256", checksum)
            .body(Body::from(bytes))
            .unwrap()
    };
    assert_eq!(
        routes(f.app.clone(), "/none")
            .oneshot(request(&hash(data), b"bad_data"))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(f.core().chunks(uid).unwrap().len(), 0);
    {
        let mut c = f.core();
        c.inflight.insert((uid.to_owned(), 1));
        c.inflight.insert((uid.to_owned(), 2));
    }
    assert_eq!(
        routes(f.app.clone(), "/none")
            .oneshot(request(&hash(data), data))
            .await
            .unwrap()
            .status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    {
        let mut c = f.core();
        c.inflight.clear();
        c.db.execute("UPDATE uploads SET expires=?1", [now() - 1])
            .unwrap();
        c.maintain().unwrap();
        assert_eq!(c.upload(uid).unwrap().state, "expired");
        assert!(!c.storage.exists(&format!(".magic-cloud/uploads/{uid}")));
    }
    let (_, _, v) = f
        .request(
            "POST",
            "/api/uploads",
            json!({"parent":"root","name":"empty","size":0,"digest":hash([])}),
            Some(&cookie),
            Some(&csrf),
        )
        .await;
    assert_eq!(
        f.request(
            "POST",
            &format!(
                "/api/uploads/{}/complete",
                v["upload"]["id"].as_str().unwrap()
            ),
            json!({}),
            Some(&cookie),
            Some(&csrf)
        )
        .await
        .0,
        StatusCode::OK
    );
}
#[test]
fn password_generation_invalidates_sessions_on_reopen() {
    let f = Fixture::new();
    let Fixture { dir, app } = f;
    {
        let c = app.core.lock().unwrap();
        c.db.execute(
            "INSERT INTO sessions VALUES('old',?1,?2,'csrf')",
            params![now() + 604800, c.generation],
        )
        .unwrap();
    }
    drop(app);
    let storage = storage::Storage::open(dir.path().to_str().unwrap(), "test-disk", false).unwrap();
    let c = Core::open(
        storage,
        "test-session-secret-at-least-32-characters",
        "changed-test-password",
    )
    .unwrap();
    assert_eq!(
        c.db.query_row("SELECT count(*) FROM sessions", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn spa_deep_links_serve_html_but_unknown_api_does_not() {
    let f = Fixture::new();
    let static_dir = tempfile::tempdir().unwrap();
    fs::write(
        static_dir.path().join("index.html"),
        "<!doctype html><div id=app></div>",
    )
    .unwrap();
    for path in ["/s/test-token", "/files/root", "/settings"] {
        let req = axum::http::Request::builder()
            .uri(path)
            .body(Body::empty())
            .unwrap();
        let res = routes(f.app.clone(), static_dir.path().to_str().unwrap())
            .oneshot(req)
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert!(
            res.headers()["content-type"]
                .to_str()
                .unwrap()
                .contains("text/html")
        );
    }
    assert_eq!(
        f.request("GET", "/api/unknown", json!({}), None, None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
}

#[test]
fn cannot_overwrite_ancestor_containing_source() {
    let f = Fixture::new();
    let mut c = f.core();
    let folder = c.mkdir("root", "A").unwrap();
    let file = add_file(&mut c, &folder.id, "child", b"preserve");
    assert_eq!(
        c.relocate(&file.id, "root", "A", "overwrite")
            .unwrap_err()
            .1,
        "overwrite_source_ancestor"
    );
    assert!(!c.broken);
    c.verify(&file).unwrap();
}

#[test]
fn initialized_storage_with_empty_database_is_refused() {
    let f = Fixture::new();
    let Fixture { dir, app } = f;
    drop(app);
    let dbpath = dir.path().join(".magic-cloud/database/magic-cloud.db");
    fs::write(dbpath, []).unwrap();
    let storage = storage::Storage::open(dir.path().to_str().unwrap(), "test-disk", false).unwrap();
    assert_eq!(
        Core::open(storage, "secret", "password").err().unwrap().1,
        "database_uninitialized_manual_recovery_required"
    );
}

#[tokio::test]
async fn restoring_purged_item_is_not_found_without_stopping_service() {
    let f = Fixture::new();
    let n = add_file(&mut f.core(), "root", "purged", b"data");
    {
        let mut c = f.core();
        let mut op = Mutation::default();
        c.trash_plan(&n, &mut op).unwrap();
        c.commit(op).unwrap();
        c.purge(&n.id).unwrap();
    }
    let (cookie, csrf) = f.auth().await;
    assert_eq!(
        f.request(
            "POST",
            &format!("/api/trash/{}/restore", n.id),
            json!({}),
            Some(&cookie),
            Some(&csrf)
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        f.request("GET", "/api/settings", json!({}), Some(&cookie), None)
            .await
            .0,
        StatusCode::OK
    );
}
