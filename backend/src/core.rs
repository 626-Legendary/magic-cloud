use crate::{
    error::{Error, Result},
    storage::{Storage, name},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    io::Read,
    os::unix::fs::MetadataExt,
    time::{SystemTime, UNIX_EPOCH},
};
pub const CHUNK: u64 = 16 * 1024 * 1024;
pub const MAX_FILE: u64 = 10_000_000_000;
pub const LOGIN_AUDIT_KEEP: i64 = 50;
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
pub fn token() -> String {
    use rand::RngCore;
    let mut b = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut b);
    hex::encode(b)
}
pub fn hash(b: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(b))
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub parent: Option<String>,
    pub name: String,
    pub path: String,
    pub kind: String,
    pub size: u64,
    pub mtime: i64,
    pub mtime_ns: i64,
    pub dev: u64,
    pub ino: u64,
    pub deleted: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Upload {
    pub id: String,
    pub parent: String,
    pub name: String,
    pub size: u64,
    pub digest: String,
    pub strategy: String,
    pub expires: i64,
    pub state: String,
    pub node: Option<String>,
}
#[derive(Serialize, Deserialize)]
pub struct Move {
    pub src: String,
    pub dst: String,
    pub dev: u64,
    pub ino: u64,
}
#[derive(Default, Serialize, Deserialize)]
pub struct Mutation {
    pub moves: Vec<Move>,
    pub nodes: Vec<Node>,
    pub trash: Vec<Value>,
    pub clear_trash: Vec<String>,
    pub revoke: Vec<String>,
    pub upload: Option<(String, String)>,
}
pub struct Core {
    pub storage: Storage,
    pub db: Connection,
    pub _instance_lock: std::fs::File,
    pub generation: String,
    pub token_key: [u8; 32],
    pub inflight: HashSet<(String, u64)>,
    pub attempts: HashMap<String, Vec<i64>>,
    pub broken: bool,
    pub reserve_bytes: u64,
    pub reserve_percent: u64,
}
fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Node> {
    Ok(Node {
        id: r.get(0)?,
        parent: r.get(1)?,
        name: r.get(2)?,
        path: r.get(3)?,
        kind: r.get(4)?,
        size: r.get::<_, i64>(5)? as u64,
        mtime: r.get(6)?,
        mtime_ns: r.get(10)?,
        dev: r.get::<_, i64>(7)? as u64,
        ino: r.get::<_, i64>(8)? as u64,
        deleted: r.get(9)?,
    })
}
impl Core {
    pub fn open(storage: Storage, secret: &str, password: &str) -> Result<Self> {
        let dbpath = format!("{}/.magic-cloud/database/magic-cloud.db", storage.path);
        if !storage.exists(".magic-cloud/database/magic-cloud.db")
            && (std::fs::read_dir(format!("{}/files", storage.path))?
                .next()
                .is_some()
                || storage.exists(".magic-cloud/initialized"))
        {
            return Err(Error::bad("database_missing_manual_recovery_required"));
        }
        use std::os::fd::AsRawFd;
        let instance_lock = storage.file(".magic-cloud/app.lock", libc::O_RDWR | libc::O_CREAT)?;
        if unsafe { libc::flock(instance_lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(Error::bad("another_instance_running"));
        }
        let db = Connection::open_with_flags(
            dbpath,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
                | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
                | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;",
        )?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 0 && storage.exists(".magic-cloud/initialized") {
            return Err(Error::bad(
                "database_uninitialized_manual_recovery_required",
            ));
        }
        if version > 1 {
            return Err(Error::bad("unsupported_database_version"));
        }
        db.execute_batch("BEGIN IMMEDIATE;")?;
        db.execute_batch(include_str!("../migrations/001.sql"))?;
        db.execute_batch("PRAGMA user_version=1; COMMIT;")?;
        let generation = hash(format!("{secret}\0{password}"));
        let mut c = Self {
            storage,
            db,
            _instance_lock: instance_lock,
            generation,
            token_key: Sha256::digest(secret.as_bytes()).into(),
            inflight: HashSet::new(),
            attempts: HashMap::new(),
            broken: false,
            reserve_bytes: 2 * 1024 * 1024 * 1024,
            reserve_percent: 5,
        };
        let disk: Option<String> =
            c.db.query_row("SELECT value FROM config WHERE key='storage_id'", [], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(disk) = disk {
            if disk != c.storage.marker {
                return Err(Error::bad("database_storage_mismatch"));
            }
        } else {
            c.db.execute(
                "INSERT INTO config VALUES('storage_id',?1)",
                [&c.storage.marker],
            )?;
        }
        c.db.execute(
            "DELETE FROM sessions WHERE generation<>?1 OR expires<=?2",
            params![c.generation, now()],
        )?;
        if c.db
            .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get::<_, i64>(0))?
            == 0
        {
            let m = c.storage.metadata("files")?;
            let n = Node {
                id: "root".into(),
                parent: None,
                name: "".into(),
                path: "files".into(),
                kind: "folder".into(),
                size: 0,
                mtime: m.mtime(),
                mtime_ns: m.mtime_nsec(),
                dev: m.dev(),
                ino: m.ino(),
                deleted: false,
            };
            c.put_node(&n)?;
        }
        c.storage
            .file(".magic-cloud/initialized", libc::O_WRONLY | libc::O_CREAT)?
            .sync_all()?;
        c.storage
            .file(".magic-cloud", libc::O_RDONLY | libc::O_DIRECTORY)?
            .sync_all()?;
        c.recover()?;
        c.prune_login_audit()?;
        Ok(c)
    }
    pub fn guard(&mut self) -> Result<()> {
        if self.broken {
            return Err(Error::bad("recovery_required"));
        }
        if let Err(e) = self.storage.check() {
            self.broken = true;
            return Err(e);
        }
        Ok(())
    }
    pub fn audit(&self, success: bool, ip: &str, category: &str) -> Result<()> {
        self.db.execute(
            "INSERT INTO login_audit(time,success,ip,category) VALUES(?1,?2,?3,?4)",
            params![now(), success, ip, category],
        )?;
        self.prune_login_audit()?;
        Ok(())
    }
    fn prune_login_audit(&self) -> Result<()> {
        self.db.execute(
            "DELETE FROM login_audit WHERE id NOT IN(SELECT id FROM login_audit ORDER BY id DESC LIMIT ?1)",
            [LOGIN_AUDIT_KEEP],
        )?;
        Ok(())
    }
    pub fn put_node(&self, n: &Node) -> Result<()> {
        self.db.execute("INSERT INTO nodes VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) ON CONFLICT(id) DO UPDATE SET parent=excluded.parent,name=excluded.name,path=excluded.path,kind=excluded.kind,size=excluded.size,mtime=excluded.mtime,dev=excluded.dev,ino=excluded.ino,deleted=excluded.deleted,mtime_ns=excluded.mtime_ns",params![n.id,n.parent,n.name,n.path,n.kind,n.size as i64,n.mtime,n.dev as i64,n.ino as i64,n.deleted,n.mtime_ns])?;
        Ok(())
    }
    pub fn node(&self, id: &str) -> Result<Node> {
        self.db
            .query_row("SELECT * FROM nodes WHERE id=?1", [id], row)
            .optional()?
            .ok_or_else(Error::missing)
    }
    pub fn open_node(&self, n: &Node) -> Result<std::fs::File> {
        if n.deleted {
            return Err(Error::missing());
        }
        let file = self
            .storage
            .file(&n.path, libc::O_RDONLY | libc::O_NONBLOCK)?;
        let m = file.metadata()?;
        if m.dev() != n.dev
            || m.ino() != n.ino
            || (n.kind == "file"
                && (m.len() != n.size || m.mtime() != n.mtime || m.mtime_nsec() != n.mtime_ns))
            || m.is_dir() != (n.kind == "folder")
        {
            return Err(Error::bad("file_identity_changed"));
        }
        Ok(file)
    }
    pub fn verify(&self, n: &Node) -> Result<()> {
        self.open_node(n).map(|_| ())
    }
    pub fn folder(&self, id: &str) -> Result<Node> {
        let n = self.node(id)?;
        self.verify(&n)?;
        if n.kind != "folder" {
            return Err(Error::bad("folder_required"));
        }
        Ok(n)
    }
    pub fn nodes(&self) -> Result<Vec<Node>> {
        Ok(self
            .db
            .prepare("SELECT * FROM nodes")?
            .query_map([], row)?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn subtree(&self, n: &Node) -> Result<Vec<Node>> {
        let all = self.nodes()?;
        let mut result = vec![n.clone()];
        let mut ids = HashSet::from([n.id.clone()]);
        loop {
            let additions: Vec<Node> = all
                .iter()
                .filter(|x| {
                    x.deleted == n.deleted
                        && !ids.contains(&x.id)
                        && x.parent.as_ref().is_some_and(|p| ids.contains(p))
                })
                .filter(|x| {
                    !n.deleted
                        || !self
                            .db
                            .query_row(
                                "SELECT EXISTS(SELECT 1 FROM trash WHERE id=?1)",
                                [&x.id],
                                |r| r.get::<_, bool>(0),
                            )
                            .unwrap_or(true)
                })
                .cloned()
                .collect();
            if additions.is_empty() {
                break;
            }
            for x in additions {
                ids.insert(x.id.clone());
                result.push(x)
            }
        }
        Ok(result)
    }
    pub fn seal_token(&self, t: &str) -> Result<String> {
        use chacha20poly1305::{ChaCha20Poly1305, KeyInit, aead::Aead};
        use rand::RngCore;
        let cipher = ChaCha20Poly1305::new((&self.token_key).into());
        let mut nonce = [0u8; 12];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let encrypted = cipher
            .encrypt((&nonce).into(), t.as_bytes())
            .map_err(|_| Error::bad("token_encryption_error"))?;
        Ok(format!("{}{}", hex::encode(nonce), hex::encode(encrypted)))
    }
    pub fn unseal_token(&self, t: &str) -> Result<String> {
        use chacha20poly1305::{ChaCha20Poly1305, KeyInit, aead::Aead};
        let b = hex::decode(t).map_err(|_| Error::bad("token_decryption_error"))?;
        if b.len() < 12 {
            return Err(Error::bad("token_decryption_error"));
        }
        let cipher = ChaCha20Poly1305::new((&self.token_key).into());
        let text = cipher
            .decrypt((&b[..12]).into(), &b[12..])
            .map_err(|_| Error::bad("token_decryption_error"))?;
        String::from_utf8(text).map_err(|_| Error::bad("token_decryption_error"))
    }
    pub fn within(&self, node: &str, root: &str) -> Result<bool> {
        let mut n = self.node(node)?;
        for _ in 0..1024 {
            if n.deleted {
                return Ok(false);
            }
            if n.id == root {
                return Ok(true);
            }
            match n.parent {
                Some(ref p) => n = self.node(p)?,
                None => return Ok(false),
            }
        }
        Ok(false)
    }
    pub fn destination(
        &self,
        parent: &Node,
        wanted: &str,
        strategy: &str,
        except: Option<&str>,
    ) -> Result<(String, Option<Node>)> {
        name(wanted)?;
        if !["ask", "overwrite", "rename", "cancel"].contains(&strategy) {
            return Err(Error::bad("invalid_strategy"));
        }
        let siblings: Vec<Node> = self
            .nodes()?
            .into_iter()
            .filter(|n| {
                !n.deleted
                    && n.parent.as_deref() == Some(&parent.id)
                    && Some(n.id.as_str()) != except
            })
            .collect();
        let old = siblings.iter().find(|n| n.name == wanted).cloned();
        if let Some(old) = old {
            match strategy {
                "overwrite" => {
                    self.verify(&old)?;
                    return Ok((wanted.into(), Some(old)));
                }
                "rename" => {
                    let (stem, ext) = wanted
                        .rsplit_once('.')
                        .filter(|(s, _)| !s.is_empty())
                        .map(|(s, e)| (s, format!(".{e}")))
                        .unwrap_or((wanted, String::new()));
                    for i in 1..10000 {
                        let candidate = format!("{stem} ({i}){ext}");
                        if !siblings.iter().any(|n| n.name == candidate) {
                            name(&candidate)?;
                            return Ok((candidate, None));
                        }
                    }
                    return Err(Error::conflict());
                }
                _ => return Err(Error::conflict()),
            }
        }
        Ok((wanted.into(), None))
    }
    pub fn move_item(&self, n: &Node, dst: String) -> Move {
        Move {
            src: n.path.clone(),
            dst,
            dev: n.dev,
            ino: n.ino,
        }
    }
    pub fn trash_plan(&self, n: &Node, m: &mut Mutation) -> Result<()> {
        if n.id == "root" {
            return Err(Error::bad("root_protected"));
        }
        self.verify(n)?;
        let path = format!(".magic-cloud/trash/{}", n.id);
        m.moves.push(self.move_item(n, path.clone()));
        for mut x in self.subtree(n)? {
            x.deleted = true;
            m.revoke.push(x.id.clone());
            m.nodes.push(x)
        }
        m.trash.push(
            json!({"id":n.id,"parent":n.parent,"name":n.name,"path":path,"deleted_at":now()}),
        );
        Ok(())
    }
    pub fn commit(&mut self, m: Mutation) -> Result<()> {
        let oid = id();
        let data = serde_json::to_string(&m).map_err(|_| Error::bad("journal_error"))?;
        self.db
            .execute("INSERT INTO operations VALUES(?1,?2)", params![oid, data])?;
        if let Err(e) = self.apply(&oid, &m) {
            self.broken = true;
            return Err(e);
        }
        Ok(())
    }
    fn apply(&mut self, oid: &str, m: &Mutation) -> Result<()> {
        for mv in &m.moves {
            let src = self.storage.metadata(&mv.src).ok();
            let dst = self.storage.metadata(&mv.dst).ok();
            let matches = |x: &std::fs::Metadata| x.dev() == mv.dev && x.ino() == mv.ino;
            if dst.as_ref().is_some_and(matches)
                && (src.is_none()
                    || m.moves.iter().any(|other| {
                        other.dst == mv.src
                            && src
                                .as_ref()
                                .is_some_and(|s| s.dev() == other.dev && s.ino() == other.ino)
                    }))
            {
                continue;
            }
            if src.as_ref().is_some_and(matches) && dst.is_none() {
                self.storage.rename(&mv.src, &mv.dst)?
            } else {
                return Err(Error::bad("operation_identity_mismatch"));
            }
        }
        self.db.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> Result<()> {
            // Release old live-name constraints before installing the final subtree.
            for n in &m.nodes {
                self.db
                    .execute("UPDATE nodes SET deleted=1 WHERE id=?1", [&n.id])?;
            }
            for n in &m.nodes {
                self.put_node(n)?;
            }
            for t in &m.trash {
                self.db.execute(
                    "INSERT INTO trash VALUES(?1,?2,?3,?4,?5)",
                    params![
                        t["id"].as_str(),
                        t["parent"].as_str(),
                        t["name"].as_str(),
                        t["path"].as_str(),
                        t["deleted_at"].as_i64()
                    ],
                )?;
            }
            for id in &m.clear_trash {
                self.db.execute("DELETE FROM trash WHERE id=?1", [id])?;
            }
            for id in &m.revoke {
                self.db
                    .execute("UPDATE shares SET revoked=1 WHERE root=?1", [id])?;
            }
            if let Some((u, n)) = &m.upload {
                self.db.execute(
                    "UPDATE uploads SET state='complete',node=?2 WHERE id=?1",
                    params![u, n],
                )?;
            }
            self.db
                .execute("DELETE FROM operations WHERE id=?1", [oid])?;
            Ok(())
        })();
        match result {
            Ok(()) => self.db.execute_batch("COMMIT")?,
            Err(e) => {
                let _ = self.db.execute_batch("ROLLBACK");
                return Err(e);
            }
        }
        Ok(())
    }
    pub fn recover(&mut self) -> Result<()> {
        let ops = self
            .db
            .prepare("SELECT id,payload FROM operations ORDER BY rowid")?
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for (oid, p) in ops {
            self.apply(
                &oid,
                &serde_json::from_str(&p).map_err(|_| Error::bad("corrupt_operation"))?,
            )?
        }
        Ok(())
    }
    pub fn mkdir(&mut self, parent: &str, n: &str) -> Result<Node> {
        let p = self.folder(parent)?;
        let (n, _) = self.destination(&p, n, "ask", None)?;
        let uid = id();
        let temp = format!(".magic-cloud/operations/{uid}");
        self.storage.mkdir(&temp)?;
        let meta = self.storage.metadata(&temp)?;
        let node = Node {
            id: uid,
            parent: Some(p.id),
            name: n.clone(),
            path: format!("{}/{n}", p.path),
            kind: "folder".into(),
            size: 0,
            mtime: meta.mtime(),
            mtime_ns: meta.mtime_nsec(),
            dev: meta.dev(),
            ino: meta.ino(),
            deleted: false,
        };
        let m = Mutation {
            moves: vec![Move {
                src: temp,
                dst: node.path.clone(),
                dev: node.dev,
                ino: node.ino,
            }],
            nodes: vec![node.clone()],
            ..Default::default()
        };
        self.commit(m)?;
        Ok(node)
    }
    pub fn relocate(
        &mut self,
        id: &str,
        parent: &str,
        wanted: &str,
        strategy: &str,
    ) -> Result<Node> {
        let n = self.node(id)?;
        self.verify(&n)?;
        if n.id == "root" {
            return Err(Error::bad("root_protected"));
        }
        let p = self.folder(parent)?;
        if self.within(&p.id, &n.id)? {
            return Err(Error::bad("move_into_descendant"));
        }
        let (name, old) = self.destination(&p, wanted, strategy, Some(id))?;
        let dst = format!("{}/{name}", p.path);
        if dst == n.path {
            return Ok(n);
        }
        let mut m = Mutation::default();
        if let Some(old) = old {
            if self.within(&n.id, &old.id)? {
                return Err(Error::bad("overwrite_source_ancestor"));
            }
            self.trash_plan(&old, &mut m)?
        }
        m.moves.push(self.move_item(&n, dst.clone()));
        for mut x in self.subtree(&n)? {
            x.path = format!("{}{}", dst, &x.path[n.path.len()..]);
            if x.id == n.id {
                x.parent = Some(p.id.clone());
                x.name = name.clone()
            }
            m.nodes.push(x)
        }
        self.commit(m)?;
        self.node(id)
    }
    pub fn upload(&self, id: &str) -> Result<Upload> {
        self.db
            .query_row("SELECT * FROM uploads WHERE id=?1", [id], |r| {
                Ok(Upload {
                    id: r.get(0)?,
                    parent: r.get(1)?,
                    name: r.get(2)?,
                    size: r.get::<_, i64>(3)? as u64,
                    digest: r.get(4)?,
                    strategy: r.get(5)?,
                    expires: r.get(6)?,
                    state: r.get(7)?,
                    node: r.get(8)?,
                })
            })
            .optional()?
            .ok_or_else(Error::missing)
    }
    pub fn pending(&self, u: &Upload) -> Result<()> {
        if u.expires <= now() || u.state != "pending" {
            return Err(Error::bad("upload_inactive"));
        }
        Ok(())
    }
    pub fn chunks(&self, id: &str) -> Result<Vec<Value>> {
        Ok(self.db.prepare("SELECT idx,digest,size FROM chunks WHERE upload=?1 ORDER BY idx")?.query_map([id],|r|Ok(json!({"index":r.get::<_,i64>(0)?,"digest":r.get::<_,String>(1)?,"size":r.get::<_,i64>(2)?})))?.collect::<std::result::Result<Vec<_>,_>>()?)
    }
    pub fn reserve(&self, size: u64) -> Result<()> {
        let (total, free) = self.storage.space()?;
        let reserved: i64 = self.db.query_row(
            "SELECT COALESCE(SUM(size),0) FROM uploads WHERE state='pending' AND expires>?1",
            [now()],
            |r| r.get(0),
        )?;
        let allocated:i64=self.db.query_row("SELECT COALESCE(SUM(c.size),0) FROM chunks c JOIN uploads u ON u.id=c.upload WHERE u.state='pending' AND u.expires>?1",[now()],|r|r.get(0))?;
        let need = (reserved - allocated).max(0) as u64
            + size
            + self.reserve_bytes.max(total * self.reserve_percent / 100);
        if need > free {
            return Err(Error(
                axum::http::StatusCode::INSUFFICIENT_STORAGE,
                "insufficient_space",
                Value::Null,
            ));
        }
        Ok(())
    }
    pub fn complete(&mut self, id: &str) -> Result<Node> {
        let u = self.upload(id)?;
        if u.state == "complete" {
            return self.node(u.node.as_deref().unwrap_or(""));
        }
        self.pending(&u)?;
        if self.inflight.iter().any(|(x, _)| x == id) {
            return Err(Error::bad("chunks_in_flight"));
        }
        let chunks = self.chunks(id)?;
        if chunks.len() as u64 != u.size.div_ceil(CHUNK) {
            return Err(Error::bad("incomplete_upload"));
        }
        let temp = format!(".magic-cloud/uploads/{id}");
        let mut f = self.storage.file(&temp, libc::O_RDONLY)?;
        let mut h = Sha256::new();
        let mut b = vec![0u8; 1024 * 1024];
        loop {
            let len = f.read(&mut b)?;
            if len == 0 {
                break;
            }
            h.update(&b[..len]);
        }
        if f.metadata()?.len() != u.size || hex::encode(h.finalize()) != u.digest {
            return Err(Error::bad("file_checksum_mismatch"));
        }
        f.sync_all()?;
        let p = self.folder(&u.parent)?;
        let (name, old) = self.destination(&p, &u.name, &u.strategy, None)?;
        let mut m = Mutation::default();
        if let Some(old) = old {
            self.trash_plan(&old, &mut m)?
        }
        let meta = f.metadata()?;
        let n = Node {
            id: crate::core::id(),
            parent: Some(p.id),
            name: name.clone(),
            path: format!("{}/{name}", p.path),
            kind: "file".into(),
            size: u.size,
            mtime: meta.mtime(),
            mtime_ns: meta.mtime_nsec(),
            dev: meta.dev(),
            ino: meta.ino(),
            deleted: false,
        };
        m.moves.push(Move {
            src: temp,
            dst: n.path.clone(),
            dev: n.dev,
            ino: n.ino,
        });
        m.nodes.push(n.clone());
        m.upload = Some((id.into(), n.id.clone()));
        self.commit(m)?;
        Ok(n)
    }
    pub fn backup(&self) -> Result<String> {
        let n = format!("{}-{}.db", now(), id());
        let p = format!("{}/.magic-cloud/backups/{n}", self.storage.path);
        let mut dst = Connection::open(p)?;
        rusqlite::backup::Backup::new(&self.db, &mut dst)?.run_to_completion(
            128,
            std::time::Duration::from_millis(10),
            None,
        )?;
        self.storage
            .file(&format!(".magic-cloud/backups/{n}"), libc::O_RDONLY)?
            .sync_all()?;
        self.storage
            .file(".magic-cloud/backups", libc::O_RDONLY | libc::O_DIRECTORY)?
            .sync_all()?;
        Ok(n)
    }
    pub fn restore(&mut self, id: &str, parent: Option<&str>, strategy: &str) -> Result<Node> {
        let mut n = self.node(id)?;
        if !n.deleted {
            return Err(Error::bad("not_in_trash"));
        }
        let (original, path): (String, String) = self
            .db
            .query_row("SELECT parent,path FROM trash WHERE id=?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?
            .ok_or_else(Error::missing)?;
        let p = self.folder(parent.unwrap_or(&original))?;
        let (name, old) = self.destination(&p, &n.name, strategy, None)?;
        let dst = format!("{}/{name}", p.path);
        let meta = self.storage.metadata(&path)?;
        if meta.dev() != n.dev || meta.ino() != n.ino {
            return Err(Error::bad("file_identity_changed"));
        }
        let mut m = Mutation::default();
        if let Some(old) = old {
            self.trash_plan(&old, &mut m)?
        }
        m.moves.push(Move {
            src: path,
            dst: dst.clone(),
            dev: n.dev,
            ino: n.ino,
        });
        for mut x in self.subtree(&n)? {
            x.path = format!("{}{}", dst, &x.path[n.path.len()..]);
            x.deleted = false;
            if x.id == id {
                x.parent = Some(p.id.clone());
                x.name = name.clone()
            }
            m.nodes.push(x)
        }
        m.clear_trash.push(id.into());
        self.commit(m)?;
        n = self.node(id)?;
        Ok(n)
    }
    pub fn purge(&mut self, id: &str) -> Result<()> {
        let path: String = self
            .db
            .query_row("SELECT path FROM trash WHERE id=?1", [id], |r| r.get(0))
            .optional()?
            .ok_or_else(Error::missing)?;
        if self.storage.exists(&path) {
            self.storage.remove(&path)?
        }
        self.db.execute("DELETE FROM trash WHERE id=?1", [id])?;
        Ok(())
    }
    pub fn maintain(&mut self) -> Result<()> {
        self.guard()?;
        let expired = self
            .db
            .prepare("SELECT id FROM uploads WHERE expires<=?1 AND state='pending'")?
            .query_map([now()], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for id in expired {
            if self.inflight.iter().any(|(x, _)| *x == id) {
                continue;
            }
            let p = format!(".magic-cloud/uploads/{id}");
            if self.storage.exists(&p) {
                self.storage.remove(&p)?
            }
            self.db
                .execute("UPDATE uploads SET state='expired' WHERE id=?1", [id])?;
        }
        let expired = self
            .db
            .prepare("SELECT id FROM trash WHERE deleted_at<=?1")?
            .query_map([now() - 30 * 86400], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for id in expired {
            self.purge(&id)?;
        }
        self.db
            .execute("DELETE FROM sessions WHERE expires<=?1", [now()])?;
        Ok(())
    }
}

pub fn diagnose(storage: &Storage) -> Result<Value> {
    let db = Connection::open_with_flags(
        format!("{}/.magic-cloud/database/magic-cloud.db", storage.path),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    let integrity: String = db.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    let pending: i64 = db.query_row("SELECT COUNT(*) FROM operations", [], |r| r.get(0))?;
    let nodes = db
        .prepare("SELECT * FROM nodes WHERE deleted=0")?
        .query_map([], row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mismatches = nodes
        .iter()
        .filter(|n| {
            !storage.metadata(&n.path).is_ok_and(|m| {
                m.dev() == n.dev
                    && m.ino() == n.ino
                    && (n.kind == "folder"
                        || (m.len() == n.size
                            && m.mtime() == n.mtime
                            && m.mtime_nsec() == n.mtime_ns))
            })
        })
        .count();
    Ok(
        json!({"integrity":integrity,"pending_operations":pending,"live_nodes":nodes.len(),"identity_mismatches":mismatches,"safe_for_manual_review":integrity=="ok"&&pending==0&&mismatches==0}),
    )
}
