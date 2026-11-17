use crate::*;
use std::{
    io::Read,
    os::unix::process::CommandExt,
    process::{Command, Stdio},
    sync::OnceLock,
};
static PERMIT: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
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
        _ => None,
    }
}
fn render(input: std::fs::File, format: &str) -> Result<Vec<u8>> {
    let mut command = Command::new("ffmpeg");
    command
        .args([
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
            "pipe",
            "-f",
            format,
            "-i",
            "pipe:0",
            "-frames:v",
            "1",
            "-vf",
            "scale=320:240:force_original_aspect_ratio=decrease",
            "-an",
            "-f",
            "image2pipe",
            "-vcodec",
            "mjpeg",
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
    let (key, source, existing, fmt) = a
        .run(move |c| {
            let n = c.node(&id)?;
            c.verify(&n)?;
            let fmt = format(&n.name).ok_or_else(Error::missing)?;
            if n.size > 512 * 1024 * 1024 {
                return Err(Error::bad("thumbnail_input_limit"));
            }
            let key = hash(format!(
                "{}:{}:{}:{}:{}:{}",
                n.id, n.dev, n.ino, n.size, n.mtime, n.mtime_ns
            ));
            let path = format!(".magic-cloud/thumbnails/{key}.jpg");
            let failure = format!(".magic-cloud/thumbnails/{key}.failed");
            if c.storage.exists(&failure) {
                let mut text = String::new();
                c.storage
                    .file(&failure, libc::O_RDONLY)?
                    .read_to_string(&mut text)?;
                if text.parse::<u32>().unwrap_or(3) >= 3 {
                    return Err(Error::bad("thumbnail_retry_exhausted"));
                }
            }
            let existing = if c.storage.exists(&path) {
                Some(c.storage.file(&path, libc::O_RDONLY)?)
            } else {
                None
            };
            Ok((key, c.open_node(&n)?, existing, fmt))
        })
        .await?;
    if let Some(f) = existing {
        return image_response(f).await;
    }
    let permit = PERMIT
        .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(1)))
        .clone()
        .try_acquire_owned()
        .map_err(|_| Error(StatusCode::TOO_MANY_REQUESTS, "thumbnail_busy", Value::Null))?;
    let rendered = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        render(source, fmt)
    })
    .await
    .map_err(|_| Error::bad("thumbnail_failed"))?;
    let data = match rendered {
        Ok(data) => data,
        Err(error) => {
            let failed_key = key.clone();
            let _ = a
                .run(move |c| {
                    use std::io::Write;
                    let path = format!(".magic-cloud/thumbnails/{failed_key}.failed");
                    let mut count = String::new();
                    if c.storage.exists(&path) {
                        c.storage
                            .file(&path, libc::O_RDONLY)?
                            .read_to_string(&mut count)?;
                    }
                    let count = count.parse::<u32>().unwrap_or(0) + 1;
                    let mut file = c
                        .storage
                        .file(&path, libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC)?;
                    file.write_all(count.to_string().as_bytes())?;
                    file.sync_all()?;
                    Ok(())
                })
                .await;
            return Err(error);
        }
    };
    let f = a
        .run(move |c| {
            use std::io::Write;
            let path = format!(".magic-cloud/thumbnails/{key}.jpg");
            if !c.storage.exists(&path) {
                let tmp = format!(".magic-cloud/thumbnails/{}", core::id());
                let mut f = c
                    .storage
                    .file(&tmp, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL)?;
                f.write_all(&data)?;
                f.sync_all()?;
                c.storage.rename(&tmp, &path)?;
            }
            c.storage.file(&path, libc::O_RDONLY)
        })
        .await?;
    image_response(f).await
}
async fn image_response(file: std::fs::File) -> Result<Response> {
    let mut r = Response::new(Body::from_stream(tokio_util::io::ReaderStream::new(
        tokio::fs::File::from_std(file),
    )));
    r.headers_mut()
        .insert("content-type", HeaderValue::from_static("image/jpeg"));
    Ok(r)
}
