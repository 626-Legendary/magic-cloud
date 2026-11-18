use crate::*;
use std::{
    collections::{HashSet, VecDeque},
    io::Read,
    os::unix::process::CommandExt,
    process::{Command, Stdio},
};
#[derive(Default)]
pub struct Queue {
    state: Mutex<(VecDeque<String>, HashSet<String>)>,
    notify: tokio::sync::Notify,
}
impl Queue {
    pub fn enqueue(&self, id: String) {
        if let Ok(mut state) = self.state.lock()
            && state.1.insert(id.clone())
        {
            state.0.push_back(id);
            self.notify.notify_one();
        }
    }
    fn pop(&self) -> Option<String> {
        self.state.lock().ok()?.0.pop_front()
    }
    fn finish(&self, id: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.1.remove(id);
        }
    }
}
pub async fn worker(a: App) {
    loop {
        let notified = a.thumbnails.notify.notified();
        if let Some(id) = a.thumbnails.pop() {
            let _ = generate(&a, id.clone()).await;
            a.thumbnails.finish(&id);
        } else {
            notified.await;
        }
    }
}
pub fn key(n: &Node) -> String {
    hash(format!(
        "{}:{}:{}:{}:{}:{}",
        n.id, n.dev, n.ino, n.size, n.mtime, n.mtime_ns
    ))
}
fn cached_path(c: &Core, n: &Node) -> Option<String> {
    let path = format!(".magic-cloud/thumbnails/{}.png", key(n));
    c.storage.exists(&path).then_some(path)
}
pub fn remove_cached(c: &Core, n: &Node) -> Result<()> {
    let key = key(n);
    for extension in ["png", "failed"] {
        let path = format!(".magic-cloud/thumbnails/{key}.{extension}");
        if c.storage.exists(&path) {
            c.storage.remove(&path)?;
        }
    }
    Ok(())
}
pub fn supported(name: &str) -> bool {
    format(name).is_some()
}
fn format(name: &str) -> Option<&'static str> {
    match name.rsplit('.').next()?.to_lowercase().as_str() {
        "png" => Some("png_pipe"),
        "jpg" | "jpeg" => Some("jpeg_pipe"),
        "webp" => Some("webp_pipe"),
        "gif" => Some("gif"),
        "bmp" => Some("bmp_pipe"),
        "mp4" | "m4v" | "mov" => Some("mov"),
        "mkv" | "webm" => Some("matroska"),
        "avi" => Some("avi"),
        "flv" => Some("flv"),
        "wmv" => Some("asf"),
        "ico" => Some("ico"),
        _ => None,
    }
}
fn render(input: std::fs::File, format: &str) -> Result<Vec<u8>> {
    let mut command = Command::new("ffmpeg");
    command.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-threads",
        "1",
        "-filter_threads",
        "1",
        "-max_alloc",
        "67108864",
        "-protocol_whitelist",
        "file,pipe",
        "-f",
        format,
    ]);
    if format == "mov" {
        command.args(["-enable_drefs", "0", "-use_absolute_path", "0"]);
    }
    command
        .args([
            "-i",
            "/proc/self/fd/0",
            "-frames:v",
            "1",
            "-vf",
            "scale=320:240:force_original_aspect_ratio=decrease",
            "-an",
            "-f",
            "image2pipe",
            "-vcodec",
            "png",
            "-pix_fmt",
            "rgba",
            "pipe:1",
        ])
        .stdin(Stdio::from(input))
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    unsafe {
        command.pre_exec(|| {
            for (resource, limit) in [
                (libc::RLIMIT_AS, 512 * 1024 * 1024),
                (libc::RLIMIT_CPU, 15),
                (libc::RLIMIT_FSIZE, 2 * 1024 * 1024),
            ] {
                let r = libc::rlimit {
                    rlim_cur: limit,
                    rlim_max: limit,
                };
                if libc::setrlimit(resource, &r) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().ok_or(Error::bad("thumbnail_failed"))?;
    let reader = std::thread::spawn(move || {
        let mut b = Vec::new();
        stdout
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut b)
            .map(|_| b)
    });
    let start = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            let data = reader
                .join()
                .map_err(|_| Error::bad("thumbnail_failed"))??;
            if !status.success() || data.len() > 2 * 1024 * 1024 || data.is_empty() {
                return Err(Error::bad("thumbnail_failed"));
            }
            return Ok(data);
        }
        if start.elapsed() > std::time::Duration::from_secs(20) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err(Error::bad("thumbnail_timeout"));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
pub async fn thumbnail(State(a): State<App>, Path(id): Path<String>) -> Result<Response> {
    let queued = id.clone();
    let response = a
        .run(move |c| {
            let n = c.node(&id)?;
            c.verify(&n)?;
            if !supported(&n.name) {
                return Err(Error::missing());
            }
            let key = key(&n);
            if let Some(path) = cached_path(c, &n) {
                return Ok(Some(c.storage.file(&path, libc::O_RDONLY)?));
            }
            if c.storage
                .exists(&format!(".magic-cloud/thumbnails/{key}.failed"))
            {
                return Err(Error::bad("thumbnail_failed"));
            }
            Ok(None)
        })
        .await?;
    if let Some(file) = response {
        return image_response(file).await;
    }
    a.thumbnails.enqueue(queued);
    Ok((StatusCode::ACCEPTED, Json(json!({"state":"pending"}))).into_response())
}
async fn generate(a: &App, id: String) -> Result<()> {
    let snapshot = a
        .run(move |c| {
            let n = c.node(&id)?;
            c.verify(&n)?;
            let fmt = format(&n.name).ok_or_else(Error::missing)?;
            let key = key(&n);
            if cached_path(c, &n).is_some()
                || c.storage
                    .exists(&format!(".magic-cloud/thumbnails/{key}.failed"))
            {
                return Ok(None);
            }
            Ok(Some((n.clone(), c.open_node(&n)?, fmt)))
        })
        .await?;
    let Some((node, source, fmt)) = snapshot else {
        return Ok(());
    };
    let too_large = node.size > 512 * 1024 * 1024 && !matches!(fmt, "mov" | "matroska" | "avi");
    let rendered = if too_large {
        Err(Error::bad("thumbnail_input_limit"))
    } else {
        tokio::task::spawn_blocking(move || render(source, fmt))
            .await
            .map_err(|_| Error::bad("thumbnail_failed"))?
    };
    a.run(move |c| {
        use std::io::Write;
        let current = c.node(&node.id)?;
        // A file may have been deleted or replaced while FFmpeg was running.
        if current.deleted || key(&current) != key(&node) {
            return Ok(());
        }
        c.verify(&current)?;
        let key = key(&current);
        match rendered {
            Ok(data) => {
                let path = format!(".magic-cloud/thumbnails/{key}.png");
                if !c.storage.exists(&path) {
                    let tmp = format!(".magic-cloud/thumbnails/{}.tmp", core::id());
                    let result = (|| -> Result<()> {
                        let mut file = c
                            .storage
                            .file(&tmp, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL)?;
                        file.write_all(&data)?;
                        file.sync_all()?;
                        c.storage.rename(&tmp, &path)?;
                        Ok(())
                    })();
                    if result.is_err() && c.storage.exists(&tmp) {
                        let _ = c.storage.remove(&tmp);
                    }
                    result?;
                }
            }
            Err(error) => {
                let path = format!(".magic-cloud/thumbnails/{key}.failed");
                let mut file = c
                    .storage
                    .file(&path, libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC)?;
                file.write_all(error.1.as_bytes())?;
                file.sync_all()?;
            }
        }
        Ok(())
    })
    .await
}
async fn image_response(file: std::fs::File) -> Result<Response> {
    let mut r = Response::new(Body::from_stream(tokio_util::io::ReaderStream::new(
        tokio::fs::File::from_std(file),
    )));
    r.headers_mut()
        .insert("content-type", HeaderValue::from_static("image/png"));
    Ok(r)
}

#[cfg(test)]
mod format_tests {
    use super::*;
    #[test]
    fn added_media_formats_use_explicit_demuxers() {
        assert_eq!(format("camera.FLV"), Some("flv"));
        assert_eq!(format("clip.WMV"), Some("asf"));
        assert_eq!(format("favicon.ICO"), Some("ico"));
        assert!(!supported("source.ts"));
        assert!(!supported("artwork.svg"));
    }
}
