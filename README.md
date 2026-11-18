<div align="center">
  <img src="frontend/public/Logo.png" width="96" alt="Magi, the Magic Cloud penguin">
  <h1>Magic Cloud · 麦吉云</h1>
  <p>A little penguin, guarding your digital nest.</p>
  <p><strong>English</strong> · <a href="README.zh-CN.md">简体中文</a></p>
</div>

Magic Cloud is a lightweight, single-user private cloud for the files that matter to you. Upload, organize, and share from your browser while keeping your files and database on your own server.

Photos, videos, and work files — Magi keeps them perfectly organized. Upload, organize, and share. Leave the rest to Magi.

> **Screenshot: Welcome screen**
>
> Reserved for the sign-in page and its Magi illustration.
<!-- Replace this block with: ![Magic Cloud welcome screen](docs/screenshots/welcome.png) -->

## Your files, your space

- **Organize with ease.** Folders, filename search, recent files, grid and list views, and bulk move or trash actions.
- **Upload large files.** Files up to 10 GB, chunked uploads, pause and resume, and folder uploads that preserve their structure.
- **Share a download.** Read-only links with optional expiry and revocation. Visitors see file information and a download button; file contents are never previewed on the share page.
- **See what you need.** Image previews, static video covers, audio playback where supported, and UTF-8 text/code previews. Other formats remain available to download.
- **Make it yours.** English and Chinese, light and dark themes, and a layout that adapts to desktop and mobile.
- **Keep control.** Local files, SQLite metadata, a 30-day trash, and scheduled database backups.

> **Screenshot: File library**
>
> Reserved for the running application: folders, type icons, and file cards.
<!-- Replace this block with: ![Magic Cloud file library](docs/screenshots/files.png) -->

## Requirements

- A Linux server with kernel 5.6 or newer and support for `openat2`.
- Docker Engine and the Docker Compose plugin. Node.js and Rust are only needed inside the build containers.
- A dedicated, mounted **ext4 filesystem** for files and the database. The supplied storage scripts use its filesystem UUID and a storage identity marker.
- For public access: a domain and a Cloudflare Tunnel connector token. A trusted LAN entry point can also be configured.

The application uses Vue 3, TypeScript, Rust/Axum, SQLite, and FFmpeg. Docker builds the application on the server.

## Quick start

### 1. Configure the application

From the repository directory:

```bash
cp .env.example .env
chmod 600 .env
openssl rand -hex 32
```

Use independently generated values for your password and session secret. Edit `.env`:

| Variable | Set it to |
| --- | --- |
| `APP_PASSWORD` | A unique password of at least 12 bytes |
| `SESSION_SECRET` | An independent random secret of at least 32 characters |
| `APP_ORIGIN` | Your exact public HTTPS origin, such as `https://cloud.example.com`, with no trailing slash |
| `CLOUDFLARE_TUNNEL_TOKEN` | The complete connector token, not the tunnel ID |
| `STORAGE_MOUNT` | The fixed storage mount point, such as `/mnt/magic-cloud` |
| `STORAGE_UUID` | The ext4 filesystem UUID reported by `lsblk -f` |
| `STORAGE_ID` | Fill this in after the initialization step below |

The storage scripts read plain, unquoted configuration values. Random hexadecimal credentials work well; avoid whitespace, quotes, `#`, and `$` in credentials used by those scripts. See [.env.example](.env.example) for all settings.

### 2. Mount and initialize storage

Confirm the correct ext4 filesystem with `lsblk -f`, then run:

```bash
sudo deploy/scripts/storage.sh mount
sudo deploy/scripts/storage.sh init --initialize-empty
```

Copy the printed `STORAGE_ID` into `.env`. Initialization creates the dedicated `files/` and `.magic-cloud/` directories only when both are absent. It does not format the disk, delete existing files, or import files placed there outside the application. Do not reinitialize an existing Magic Cloud storage layout.

For mounting after a server restart, add an entry to `/etc/fstab`, using your actual filesystem UUID and mount point:

```fstab
UUID=<your-ext4-uuid> /mnt/magic-cloud ext4 defaults,nofail,nodev,nosuid,noexec,x-systemd.device-timeout=10s 0 2
```

### 3. Configure the tunnel and start

In Cloudflare, route your public hostname to the HTTP service **`http://magic-cloud:5080`**. The public origin must match `APP_ORIGIN`.

```bash
sudo deploy/scripts/storage.sh check
docker compose up -d --build
docker compose ps
```

Open your configured HTTPS domain and sign in with `APP_PASSWORD`. If builds exceed the server's memory capacity, reduce Rust compilation concurrency:

```bash
docker compose build --build-arg CARGO_BUILD_JOBS=1 magic-cloud
docker compose up -d
```

### 4. Enable storage recovery

Copy the files in `deploy/systemd/` to `/etc/systemd/system/`, and update the service's `ExecStart` to the absolute repository path. Then run:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now magic-cloud-storage.timer
```

The monitor checks storage identity and recreates the application container after the correct storage becomes available again. This step matters for removable storage and recovery after a server restart. See [Operations](docs/operations.md) for failure states and manual recovery.

## Access from your LAN

Add the server's actual LAN address to `.env`:

```dotenv
PORT=5080
LAN_BIND_ADDRESS=192.168.0.101
LAN_ORIGIN=http://192.168.0.101:5080
```

Apply it:

```bash
docker compose up -d --no-deps --force-recreate magic-cloud
```

Open `http://192.168.0.101:5080`. Public and LAN entry points use the same files and password but have separate sign-in sessions. HTTP is intended for a trusted LAN; use the tunnel's HTTPS domain for public access. A share created from the LAN entry point uses that address, so the recipient must be able to reach it.

## A few useful details

| Topic | Behavior |
| --- | --- |
| Upload limit | 10,000,000,000 bytes per file; 16 MiB chunks |
| Resuming | Within 24 hours, reselect the original file and verify it to resume; uploads stop when the browser is closed |
| Trash | Files are permanently removed after 30 days |
| Backups | Consistent SQLite snapshots, normally every 24 hours; seven retained by default |
| Preview | Supported images, audio, UTF-8 text/code, and static video covers; PDF and office documents show a download option |
| Upload time | Recorded separately for new uploads; files without this value show “Not recorded” |
| Sharing | Download-only visitor pages; links can expire or be revoked |

Database backups **do not contain your files**. Keep independent copies of important files and database backups on another device. Manage files through Magic Cloud to keep its index consistent.

> **Screenshot: Sharing and settings**
>
> Reserved for the download-only visitor page or application settings.
<!-- Replace this block with: ![Magic Cloud sharing and settings](docs/screenshots/sharing.png) -->

## Update and maintain

The footer version comes from `APP_VERSION` in `.env`. It is embedded during the frontend build, so changing it requires a rebuild.

```bash
# Build and update the application after following the backup steps below.
docker compose up -d --build --no-deps magic-cloud

# Inspect service status and recent application logs.
docker compose ps
docker compose logs --tail=100 magic-cloud
```

Before upgrading, export a database backup and keep an independent copy of important files. The project uses a single current database schema; changes to that structure require a matching backup for recovery. Follow the [upgrade and recovery procedure](docs/operations.md).

## Documentation and development

- [Architecture and storage model](docs/architecture.md)
- [API reference](docs/api.md)
- [Security boundaries](docs/security.md)
- [Operations, backups, and recovery](docs/operations.md)
- [UI behavior](docs/ui-design.md)
- [Validation guide](docs/validation.md) and [acceptance checklist](docs/acceptance.md)

Run `./deploy/scripts/check.sh` for containerized formatting, tests, static checks, and a build. Browser regression tests require a disposable deployment; see the validation guide.

Cloudflare request limits and usage terms depend on your plan and configuration. Check the [Cloudflare upload limits](https://developers.cloudflare.com/network/maximum-upload-size/) for your deployment; support for large files in the application does not guarantee the same limits through every proxy.
