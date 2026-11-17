# Magic Cloud · 麦吉云

运行于 Raspberry Pi 5（8GB、64 位 Linux）的单用户私人网盘。Vue 3 + TypeScript 前端、Rust/Axum 后端、USB ext4 上的 SQLite 和原始文件；由 Cloudflare Tunnel 提供统一 HTTPS 入口。宿主机只需 Docker Engine、Compose 和系统原有的挂载/systemd 工具。

支持真实文件管理、递归名称搜索、回收站、16 MiB 分片与 24 小时断点恢复、只读文件/目录分享、预览、数据库备份、中英文、浅/深/系统主题。视频只显示静态封面与下载，无播放器。

## 首次部署

以下操作在你的 Pi 上执行。不要使用承载系统或其他应用数据的磁盘做拔盘测试。

1. 将仓库放在固定位置，例如 `/opt/magic-cloud`。`cp .env.example .env`，执行 `chmod 600 .env`。设置至少 12 字节的独立 `APP_PASSWORD`、至少 32 字符随机 `SESSION_SECRET`、准确的 HTTPS `APP_ORIGIN`（不含末尾 `/`）和完整的 Tunnel Token（不是 Tunnel ID/UUID）。可用 `openssl rand -hex 32` 本地生成密钥，不上传 `.env`。
2. 使用 `lsblk -f` 确认**已格式化为 ext4**的 USB 文件系统 UUID，将其及固定挂载点填入 `STORAGE_UUID`、`STORAGE_MOUNT`。脚本不会格式化磁盘。配置值使用不带引号的普通文本；含 `$` 的 Compose 值按 Compose 规则写为 `$$`。避免密码含空白、引号、`#` 或 `$`，可直接使用随机十六进制字符。
3. 挂载并显式初始化新的存储布局：

   ```bash
   sudo deploy/scripts/storage.sh mount
   sudo deploy/scripts/storage.sh init --initialize-empty
   ```

   初始化仅在 `files/` 和 `.magic-cloud/` 均不存在时执行；其他原始数据不会删除，也不会自动导入。将命令输出的 `STORAGE_ID` 填入 `.env`。不要对已有 Magic Cloud 存储重复初始化。默认容器 UID/GID 为 10001，初始化为该身份设置专用目录权限。
4. 建议在 `/etc/fstab` 加入下面一行，替换 UUID 与挂载点：

   ```text
   UUID=<你的UUID> /mnt/magic-cloud ext4 defaults,nofail,nodev,nosuid,noexec,x-systemd.device-timeout=10s 0 2
   ```

5. 验证并在 Pi 本机构建：

   ```bash
   sudo deploy/scripts/storage.sh check
   docker compose up -d --build
   ```

   Rust 默认两路编译；内存不足可执行 `docker compose build --build-arg CARGO_BUILD_JOBS=1 magic-cloud` 后 `docker compose up -d`。
6. 在 Cloudflare Tunnel 的连接器安装页面选择 Docker，复制安装命令 `--token` 后面的完整值，填入 `.env` 的 `CLOUDFLARE_TUNNEL_TOKEN`。仅填写 Tunnel ID（UUID）不能用于此启动方式。Compose 按 `tunnel --no-autoupdate run --token ...` 启动连接器；不要公开粘贴解析后的 Compose 配置，因为其中包含 Token。

   在 Cloudflare Tunnel 管理界面为域名绑定 HTTP 服务 `http://magic-cloud:5080`。域名必须与 `APP_ORIGIN` 完全一致；如果更改内部 `PORT`，同步修改 Tunnel 服务地址。公网流量由 Tunnel 转发；局域网入口按下文配置。为该域名禁用 HTML/API/文件的公开缓存，不使用 Cache Everything。
7. 安装 USB 恢复监控：将 `deploy/systemd/` 中两个文件复制到 `/etc/systemd/system/`，把 service 的 `ExecStart` 改为仓库实际绝对路径，然后：

   ```bash
   sudo systemctl daemon-reload
   sudo systemctl enable --now magic-cloud-storage.timer
   ```

只有手动启动 Compose 无法保证拔盘后的恢复。监控停止后端、校验 UUID/存储标记，重挂 USB 并**重新创建**容器。`cloudflared` 可继续运行。数据库异常或同一挂载反复启动失败时会保持停止，详见运维文档。

## 局域网访问

公网继续使用 `APP_ORIGIN` 的 HTTPS 域名。要同时通过树莓派 IP 访问，在 `.env` 设置：

```dotenv
PORT=5080
LAN_BIND_ADDRESS=192.168.0.101
LAN_ORIGIN=http://192.168.0.101:5080
```

将示例 IP 换成树莓派实际局域网 IP，然后执行 `docker compose up -d --build --force-recreate magic-cloud`，访问 `http://192.168.0.101:5080`。Compose 只将端口绑定到指定地址；局域网 IP 改变后需更新这两个配置并重建容器。样例配置默认仅绑定回环地址，LAN_ORIGIN 留空时关闭 HTTP 登录入口。

两个入口使用同一密码和 USB 数据，分别登录。公网会话继续使用 Secure Cookie；局域网使用独立的 host-only、HttpOnly、SameSite=Strict 会话，仍校验精确 Origin 和 CSRF。HTTP 不加密，请仅在可信局域网使用，不要将此端口做公网转发。

局域网上传、恢复与分片校验支持普通 HTTP，无需修改浏览器的安全设置。分享链接使用当前入口地址；从局域网创建的 IP 链接只能由能访问该 IP 的设备打开。断网时可继续使用局域网入口，服务仍要求正确挂载 USB。

## 使用与默认值

- 单文件最大 **10,000,000,000 字节**；分片 16 MiB，同时两个文件，每文件两个分片。开始上传前浏览器 Worker 逐块计算 SHA-256，因此大文件先显示“正在校验文件”；完成时服务端流式核验完整摘要。
- 上传引擎位于路由生命周期之外。刷新、关闭浏览器或系统休眠后不继续后台上传；24 小时内重新选择原文件并核验完整摘要可恢复。进度为服务器确认的字节数，速度为本次运行已确认数据的平均速度。
- 同名项目由用户选择覆盖、自动重命名或取消。覆盖旧项目进入回收站；回收站 30 天清理，不提供文件版本历史。
- 删除对象会撤销对象及其子树的直接分享；恢复不重启这些权限。文件移出某个分享目录后不能再使用原目录 Token 访问。重命名/移动保留对象稳定 ID。
- 最近文件根据元数据中的修改时间排序。用户直接操作 USB 文件不受支持；身份异常会拒绝访问，不自动扫描导入。
- 上传安全余量为 `max(2 GiB, 总容量 5%)`，可配置。预留不能消除运行中的磁盘满或 I/O 故障，也不会触发自动清空回收站。
- 缩略图后台生成，并发一；输入最大 512 MiB，解码内存 512 MiB、CPU 15 秒、墙钟 20 秒，输出最大 2 MiB，每个文件身份最多重试三次。超限、不支持格式或不能从流解码的媒体显示类型图标。仅允许 pipe 输入协议，不允许媒体加载额外的本地文件或网络资源。
- 文本/代码预览限 2 MiB、UTF-8，按文本转义展示；图片/音频/PDF 通过认证接口预览。
- 每日一致性 SQLite 备份，默认保留七份，可在设置创建、下载。**数据库备份不包含原始文件；同盘备份无法抵御 USB 整盘损坏。**请把重要文件及数据库另存到其他设备。

## 构建、测试和运维

```bash
# 不需要宿主机 Rust 或 Node
./deploy/scripts/check.sh
# 查看应用日志和服务状态
 docker compose logs --tail=100 magic-cloud
 docker compose ps
 sudo journalctl -u magic-cloud-storage.service -n 100
```

代码提供 [API 契约](docs/api.md)、[架构和数据模型](docs/architecture.md)、[安全边界](docs/security.md)、[升级与恢复步骤](docs/operations.md) 和 [验收清单](docs/acceptance.md)。实际执行的验证与限制记录在 [验证结果](docs/validation.md)。

Cloudflare 单次请求限制取决于账户计划，16 MiB 分片用于减少单次请求体大小；部署前应检查[官方上传限制](https://developers.cloudflare.com/network/maximum-upload-size/)及当前适用条款。流式和 Range 下载不保证 Cloudflare 公网 10 GB 下载或视频分发可用，局域网 IP 入口不依赖 Cloudflare。

三份输入需求文档只保留于本地，已排除 Git 提交。运行数据、真实秘密不应放在源码目录内。
