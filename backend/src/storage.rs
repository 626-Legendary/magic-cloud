use crate::error::{Error, Result};
use std::{
    ffi::CString,
    fs::{File, Metadata},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::Path,
};
#[derive(Debug)]
pub struct Storage {
    pub root: File,
    pub dev: u64,
    pub ino: u64,
    pub marker: String,
    pub path: String,
    pub strict: bool,
}
#[repr(C)]
struct OpenHow {
    flags: u64,
    mode: u64,
    resolve: u64,
}
pub fn name(s: &str) -> Result<()> {
    if s.is_empty()
        || s == "."
        || s == ".."
        || s == ".magic-cloud"
        || s.len() > 255
        || s.chars().any(|c| c == '/' || c == '\\' || c.is_control())
    {
        return Err(Error::bad("invalid_name"));
    }
    Ok(())
}
impl Storage {
    pub fn open_mounted(path: &str, marker: &str, external: bool) -> Result<Self> {
        // Both modes require a bind mount in production. Local directories do not
        // require a dedicated ext4 filesystem, but never use container-only data.
        Self::check_mountpoint(path)?;
        Self::open(path, marker, external)
    }
    fn check_mountpoint(path: &str) -> Result<()> {
        let mounts = std::fs::read_to_string("/proc/self/mountinfo")?;
        if !mounts
            .lines()
            .any(|l| l.split_whitespace().nth(4) == Some(path))
        {
            return Err(Error::bad("mountpoint_required"));
        }
        Ok(())
    }
    pub fn open(path: &str, marker: &str, strict: bool) -> Result<Self> {
        let root = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(path)?;
        let m = root.metadata()?;
        let s = Self {
            root,
            dev: m.dev(),
            ino: m.ino(),
            marker: marker.into(),
            path: path.into(),
            strict,
        };
        s.check()?;
        Ok(s)
    }
    pub fn file(&self, path: &str, flags: i32) -> Result<File> {
        let p = CString::new(path).map_err(|_| Error::bad("invalid_path"))?;
        let h = OpenHow {
            flags: (flags | libc::O_CLOEXEC | libc::O_NOFOLLOW) as u64,
            mode: if flags & libc::O_CREAT != 0 { 0o600 } else { 0 },
            resolve: 0x08 | 0x04 | 0x01,
        };
        let fd = unsafe {
            libc::syscall(
                libc::SYS_openat2,
                self.root.as_raw_fd(),
                p.as_ptr(),
                &h,
                std::mem::size_of::<OpenHow>(),
            )
        } as i32;
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }
    pub fn check(&self) -> Result<()> {
        let m = std::fs::symlink_metadata(&self.path)?;
        if m.dev() != self.dev || m.ino() != self.ino || !m.is_dir() {
            return Err(Error::bad("storage_identity_changed"));
        }
        if self.strict {
            let mut fs = std::mem::MaybeUninit::<libc::statfs>::uninit();
            if unsafe { libc::fstatfs(self.root.as_raw_fd(), fs.as_mut_ptr()) } != 0
                || unsafe { fs.assume_init() }.f_type != 0xef53
            {
                return Err(Error::bad("ext4_required"));
            }
            Self::check_mountpoint(&self.path)?;
        }
        for dir in [
            "files",
            ".magic-cloud",
            ".magic-cloud/database",
            ".magic-cloud/uploads",
            ".magic-cloud/trash",
            ".magic-cloud/backups",
            ".magic-cloud/thumbnails",
            ".magic-cloud/operations",
        ] {
            self.file(dir, libc::O_RDONLY | libc::O_DIRECTORY)?;
        }
        let mut marker = String::new();
        use std::io::Read;
        self.file(".magic-cloud/storage-id", libc::O_RDONLY)?
            .read_to_string(&mut marker)?;
        if marker.trim() != self.marker {
            return Err(Error::bad("storage_identity_changed"));
        }
        Ok(())
    }
    pub fn metadata(&self, path: &str) -> Result<Metadata> {
        Ok(self
            .file(path, libc::O_RDONLY | libc::O_NONBLOCK)?
            .metadata()?)
    }
    pub fn exists(&self, path: &str) -> bool {
        self.metadata(path).is_ok()
    }
    fn parent(&self, path: &str) -> Result<(File, CString)> {
        let p = Path::new(path);
        let dir = p
            .parent()
            .and_then(|x| x.to_str())
            .ok_or(Error::bad("invalid_path"))?;
        let n = CString::new(
            p.file_name()
                .and_then(|x| x.to_str())
                .ok_or(Error::bad("invalid_path"))?,
        )
        .map_err(|_| Error::bad("invalid_path"))?;
        Ok((self.file(dir, libc::O_RDONLY | libc::O_DIRECTORY)?, n))
    }
    pub fn mkdir(&self, path: &str) -> Result<()> {
        let (p, n) = self.parent(path)?;
        if unsafe { libc::mkdirat(p.as_raw_fd(), n.as_ptr(), 0o700) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        p.sync_all()?;
        Ok(())
    }
    pub fn rename(&self, src: &str, dst: &str) -> Result<()> {
        let (a, s) = self.parent(src)?;
        let (b, d) = self.parent(dst)?;
        if unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                a.as_raw_fd(),
                s.as_ptr(),
                b.as_raw_fd(),
                d.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        a.sync_all()?;
        b.sync_all()?;
        Ok(())
    }
    pub fn remove(&self, path: &str) -> Result<()> {
        let f = self.file(path, libc::O_RDONLY | libc::O_NONBLOCK)?;
        let m = f.metadata()?;
        if m.is_dir() {
            let dir = std::fs::read_dir(format!("/proc/self/fd/{}", f.as_raw_fd()))?;
            for entry in dir {
                let entry = entry?;
                let n = entry.file_name();
                let n = n.to_str().ok_or(Error::bad("invalid_name"))?;
                self.remove(&format!("{path}/{n}"))?;
            }
        }
        let (p, n) = self.parent(path)?;
        if unsafe {
            libc::unlinkat(
                p.as_raw_fd(),
                n.as_ptr(),
                if m.is_dir() { libc::AT_REMOVEDIR } else { 0 },
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        p.sync_all()?;
        Ok(())
    }
    pub fn space(&self) -> Result<(u64, u64)> {
        let mut v = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        if unsafe { libc::fstatvfs(self.root.as_raw_fd(), v.as_mut_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let v = unsafe { v.assume_init() };
        Ok((v.f_blocks * v.f_frsize, v.f_bavail * v.f_frsize))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names() {
        for n in ["../x", "..", "a/b", "a\\b", "", ".magic-cloud", "a\0b"] {
            assert!(name(n).is_err())
        }
        assert!(name("照片 (1).png").is_ok());
    }
    #[test]
    fn production_storage_requires_a_mount_in_both_modes() {
        let directory = tempfile::tempdir().unwrap();
        for external in [false, true] {
            let error =
                Storage::open_mounted(directory.path().to_str().unwrap(), "marker", external)
                    .unwrap_err();
            assert_eq!(error.1, "mountpoint_required");
        }
    }
}
