<div align="center">
  <img src="frontend/public/Logo.png" width="96" alt="麦吉云的小企鹅 Magi">
  <h1>Magic Cloud · 麦吉云</h1>
  <p>小企鹅守着，你的数字小窝。</p>
  <p><a href="README.md">English</a> · <strong>简体中文</strong></p>
</div>

麦吉云是一款轻量、简洁的单用户私有云文件管理平台。通过浏览器上传、整理和分享，让文件与数据库始终存放在你自己的服务器上。

照片、视频、工作文件，麦吉都帮你收得妥妥当当。上传、整理、分享，交给麦吉就好。

> **截图位置：欢迎页**
>
> 预留登录首页与麦吉插画的展示位置。
<!-- 替换为：![麦吉云欢迎页](docs/screenshots/welcome.png) -->

## 你的小窝，由你掌控

- **轻松整理：** 文件夹、文件名搜索、最近文件、网格与列表，以及批量移动和移入回收站。
- **大文件上传：** 单文件最大 10 GB，支持分片、暂停、断点恢复，上传文件夹时保留目录结构。
- **便捷分享：** 只读链接可设有效期、随时撤销。访客只查看文件信息并下载，分享页不预览文件内容。
- **按需预览：** 图片、静态视频封面、浏览器支持的音频，以及 UTF-8 文本和代码；其他格式可下载后打开。
- **舒适界面：** 中英文切换、浅色与深色主题，适配桌面和手机。
- **数据自主：** 本地文件、SQLite 索引、30 天回收站和定时数据库备份。

> **截图位置：文件管理**
>
> 预留实际运行界面，展示文件夹、类型图标和文件卡片。
<!-- 替换为：![麦吉云文件管理](docs/screenshots/files.png) -->

## 运行要求

- Linux 服务器，内核 5.6 或更新，支持 `openat2`。
- Docker Engine 与 Docker Compose 插件；Node.js 和 Rust 仅需在构建容器中运行。
- 一块专用、已挂载的 **ext4 文件系统**，用于文件与数据库。存储脚本通过文件系统 UUID 与存储标记识别它。
- 公网访问需要域名和 Cloudflare Tunnel 连接器令牌；也可以配置可信局域网入口。

项目使用 Vue 3、TypeScript、Rust/Axum、SQLite 和 FFmpeg，通过 Docker 在服务器上构建。

## 快速开始

### 1. 配置应用

在项目目录执行：

```bash
cp .env.example .env
chmod 600 .env
openssl rand -hex 32
```

为访问密码与会话密钥分别生成独立的随机值，然后编辑 `.env`：

| 变量 | 填写内容 |
| --- | --- |
| `APP_PASSWORD` | 至少 12 字节的独立访问密码 |
| `SESSION_SECRET` | 至少 32 字符的独立随机密钥 |
| `APP_ORIGIN` | 完整公网 HTTPS Origin，如 `https://cloud.example.com`，不要带末尾 `/` |
| `CLOUDFLARE_TUNNEL_TOKEN` | 完整连接器令牌，不是 Tunnel ID |
| `STORAGE_MOUNT` | 固定挂载点，如 `/mnt/magic-cloud` |
| `STORAGE_UUID` | `lsblk -f` 显示的 ext4 文件系统 UUID |
| `STORAGE_ID` | 完成下方初始化后再填写 |

存储脚本读取不带引号的普通配置值。随机十六进制凭据便于配置；避免在相关凭据中使用空白、引号、`#` 和 `$`。完整配置项见 [.env.example](.env.example)。

### 2. 挂载并初始化存储

使用 `lsblk -f` 确认正确的 ext4 文件系统，然后执行：

```bash
sudo deploy/scripts/storage.sh mount
sudo deploy/scripts/storage.sh init --initialize-empty
```

将输出的 `STORAGE_ID` 填入 `.env`。初始化仅在 `files/` 与 `.magic-cloud/` 均不存在时创建专用目录，不会格式化磁盘、删除已有文件或自动导入外部拷入的文件。不要对已有麦吉云存储重复初始化。

为了在服务器重启后自动挂载，在 `/etc/fstab` 中添加以下配置，替换为实际 UUID 与挂载点：

```fstab
UUID=<你的ext4文件系统UUID> /mnt/magic-cloud ext4 defaults,nofail,nodev,nosuid,noexec,x-systemd.device-timeout=10s 0 2
```

### 3. 配置隧道并启动

在 Cloudflare 中，将公网域名绑定到 HTTP 服务 **`http://magic-cloud:5080`**。公网 Origin 必须与 `APP_ORIGIN` 一致。

```bash
sudo deploy/scripts/storage.sh check
docker compose up -d --build
docker compose ps
```

打开配置的 HTTPS 域名，使用 `APP_PASSWORD` 登录。如果构建超出服务器内存容量，可以降低 Rust 编译并发：

```bash
docker compose build --build-arg CARGO_BUILD_JOBS=1 magic-cloud
docker compose up -d
```

### 4. 启用存储恢复

将 `deploy/systemd/` 中的文件复制到 `/etc/systemd/system/`，把服务的 `ExecStart` 改为项目实际绝对路径，然后执行：

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now magic-cloud-storage.timer
```

监控会校验存储身份，在正确存储恢复可用后重新创建应用容器。对于可移动存储和服务器重启后的恢复，此步骤很有必要。异常状态和人工恢复步骤见[运维文档](docs/operations.md)。

## 局域网访问

在 `.env` 中填入服务器实际局域网地址：

```dotenv
PORT=5080
LAN_BIND_ADDRESS=192.168.0.101
LAN_ORIGIN=http://192.168.0.101:5080
```

应用配置：

```bash
docker compose up -d --no-deps --force-recreate magic-cloud
```

访问 `http://192.168.0.101:5080`。公网与局域网共用文件和密码，但分别登录。HTTP 入口用于可信局域网，公网使用隧道 HTTPS 域名。从局域网入口创建的分享使用该地址，接收者必须能访问对应局域网。

## 使用说明

| 项目 | 行为 |
| --- | --- |
| 上传上限 | 单文件 10,000,000,000 字节，分片 16 MiB |
| 断点恢复 | 24 小时内重新选择并校验原文件后恢复；关闭浏览器会停止上传 |
| 回收站 | 文件保留 30 天后永久清理 |
| 数据库备份 | 默认每 24 小时生成一致性快照，保留七份 |
| 在线预览 | 支持的图片、音频、UTF-8 文本/代码及静态视频封面；PDF 和 Office 文档提供下载入口 |
| 上传时间 | 新上传文件单独记录；未记录上传时间的文件显示“未记录” |
| 文件分享 | 访客页仅提供文件信息与下载，链接可过期或撤销 |

数据库备份**不包含原始文件**。请将重要文件及数据库备份另外保存在其他设备上。通过麦吉云管理文件，保持文件与索引一致。

> **截图位置：分享与设置**
>
> 预留访客下载页或应用设置界面的展示位置。
<!-- 替换为：![麦吉云分享与设置](docs/screenshots/sharing.png) -->

## 更新与维护

Footer 的版本来自 `.env` 中的 `APP_VERSION`，在前端构建时写入，修改后需要重新构建。

```bash
# 按下方说明备份后，构建并更新应用。
docker compose up -d --build --no-deps magic-cloud

# 查看服务状态和近期日志。
docker compose ps
docker compose logs --tail=100 magic-cloud
```

升级前先导出数据库备份，并单独保存重要文件。项目只维护当前数据库结构，结构变更后的恢复需要匹配的数据备份。完整步骤见[升级与恢复](docs/operations.md)。

## 文档与开发

- [架构与存储模型](docs/architecture.md)
- [API 契约](docs/api.md)
- [安全边界](docs/security.md)
- [运维、备份与恢复](docs/operations.md)
- [界面交互规范](docs/ui-design.md)
- [验证指南](docs/validation.md)与[验收清单](docs/acceptance.md)

执行 `./deploy/scripts/check.sh`，使用容器完成格式、测试、静态检查及镜像构建。浏览器回归必须使用可丢弃部署，详见验证指南。

Cloudflare 的请求上限与使用条款取决于套餐和配置。请检查适用于当前部署的 [Cloudflare 上传限制](https://developers.cloudflare.com/network/maximum-upload-size/)；应用自身支持大文件，不代表所有代理都具有相同限制。
