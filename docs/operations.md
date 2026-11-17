# 运维、升级与恢复

## 存储状态与自动恢复

| 状态 | 行为 |
|---|---|
| 首次安装、无布局 | 显式 `init --initialize-empty`；其他路径不自动初始化 |
| Pi 重启、USB 正常 | fstab 或脚本按 UUID 挂载，systemd 校验后创建应用容器 |
| USB 不在、挂载路径为空目录 | 不通过检查，不初始化数据库；后端停止 |
| UUID/ext4/storage-id 错误 | 不启动，不接管、不格式化错误磁盘 |
| 运行中掉盘/EIO | 后端拒绝新操作并退出；宿主机停止旧容器 |
| 正确 USB 重插 | 停止应用后尝试正常卸载陈旧挂载，重新按 UUID 挂载、验证并重建容器 |
| 数据库损坏/操作身份不明 | 后端启动失败；同一挂载不反复启动，保持不可用供人工诊断 |

宿主机监控使用 flock 防止并发启动。发现不可用会清除“已尝试启动”的运行期标记；正确挂载恢复后只尝试一次启动。同一挂载上的异常退出会被锁止，避免每五秒反复创建容器。若旧挂载无法正常卸载，不强制或 lazy 卸载，应先检查占用及内核日志。

```bash
sudo deploy/scripts/storage.sh check
sudo systemctl status magic-cloud-storage.timer
sudo journalctl -u magic-cloud-storage.service -n 100
findmnt -M /mnt/magic-cloud
lsblk -f
docker compose ps -a
docker compose logs --tail=100 magic-cloud
```

确认设备、数据库与权限已修复后，手动解除启动锁止：

```bash
sudo rm -f /run/magic-cloud.last-start
sudo systemctl start magic-cloud-storage.service
```

监控配置来自仓库 `.env`，不会把它当作 shell 脚本执行；宿主机操作脚本使用简单未加引号的配置值，详见 README。不要把密码或 Token 贴到排错记录中。更改工作目录后同步修改 systemd ExecStart。

## 数据库只读诊断

先暂停恢复 timer 并停止后端，保留 cloudflared：

```bash
sudo systemctl stop magic-cloud-storage.timer
 docker compose stop magic-cloud
 docker compose run --rm --no-deps magic-cloud --diagnose
```

`--diagnose` 只读打开已有数据库，不创建布局、运行迁移或执行操作恢复，报告 integrity、待恢复操作数及活跃节点身份不一致数量。读写失败则停止，不自动修复。它不导入外部文件，也不更改分享权限。诊断时保持磁盘连接。

## 备份与人工恢复

设置可创建和导出一致性 SQLite 备份。自动备份间隔默认 86,400 秒、保留七份，通过 `BACKUP_INTERVAL_SECONDS`（最小一小时）和 `BACKUP_KEEP` 调整；保留策略在自动备份时执行，也包含手动备份。备份在后台启动后首次检查时生成，因此重启会额外产生一份。

设置页也可删除指定备份，操作前需确认永久删除风险；只删除选中的备份文件，不修改当前数据库或原始文件。

新备份名为 `magic-cloud-YYYY-MM-DD_HH-MM-SSZ-<UUID>.db`，例如 `magic-cloud-2026-11-17_12-34-56Z-<UUID>.db`。时间使用 UTC（Z），UUID 避免同一秒创建时重名。列表及自动清理按备份时间排序，兼容原来的 Unix 秒时间戳文件名；其他普通 `.db` 文件以修改时间作为回退。旧备份保留原名并继续支持下载，目录和符号链接不参与备份列表或清理。

恢复必须由管理员明确操作：

1. 停止 timer 与后端。备份整个 `.magic-cloud` 和 `files` 到另一块磁盘；不要对唯一副本做实验。
2. 选取与**同一 storage-id、同一物理文件状态**一致的数据库备份。使用运行镜像内 sqlite3 对选定备份执行 `PRAGMA integrity_check`。示例 `docker compose run --rm --no-deps --entrypoint sqlite3 magic-cloud /data/.magic-cloud/backups/<备份名>.db 'PRAGMA integrity_check;'`。
3. 仅在所有 SQLite 进程已停止后，将当前 database 目录完整移到 USB 上另一个诊断保留位置，保留原 DB/WAL/SHM；新建同权限的 database 目录，将选定备份复制为 magic-cloud.db，并恢复 APP_UID/GID、0600 权限。不要混用旧 WAL/SHM 与恢复的数据库。
4. 使用 `--diagnose` 对照实际文件身份。备份之后的移动、覆盖、删除可能导致文件状态与数据库不一致。报告异常时保持服务停止，由人工对照完整存储备份处理，不能重新分配 ID、自动重建空库或猜测路径。
5. 为避免旧备份恢复已撤销的权限，在恢复库上显式执行 `DELETE FROM sessions; UPDATE shares SET revoked=1;`。确认原始文件及操作状态后才能重新创建容器并开启 timer。

数据库缺失但 `files/` 非空或已有 initialized 标记时，启动拒绝创建新库。不要删除 initialized 标记“绕过”检查。数据库备份不包含原始文件；整盘损坏必须依靠异地完整备份恢复。

## 升级与回滚

1. 在设置导出数据库，并另存重要文件；保留当前镜像 `docker image tag magic-cloud:local magic-cloud:previous`。
2. `sudo systemctl stop magic-cloud-storage.timer`，`docker compose stop magic-cloud`，更新源码。
3. `docker compose build magic-cloud`，运行磁盘检查，再 `docker compose up -d --no-deps --force-recreate magic-cloud`。
4. 检查健康、登录、列表和小文件上传下载后启动 timer。不要同时运行两个应用实例。

回滚先停止应用与 timer，使用前一镜像。若新版本执行过不兼容迁移，必须按备份恢复流程恢复匹配数据库与文件状态，不能盲目运行旧镜像。当前版本拒绝更高数据库版本。

修改密码或 SESSION_SECRET 后运行 `docker compose up -d --no-deps --force-recreate magic-cloud`；旧会话全部失效。修改 SESSION_SECRET 会影响所有者复制已有加密分享链接，请保留原密钥或重新生成分享。

安全卸载：先 disable/stop timer，`docker compose down` 后删除 systemd 模板和源码。**不删除 USB 内容、不执行格式化**。卸载 USB 前停止后端、确认无进程占用，执行 sync 后正常 umount。

## Cloudflare 与网络

Compose 使用 `cloudflared tunnel --no-autoupdate run --token ...`。`.env` 的 `CLOUDFLARE_TUNNEL_TOKEN` 必须是控制台连接器安装命令中 `--token` 后的完整令牌，不能只填 Tunnel ID（UUID）。原来的 TUNNEL_TOKEN 环境变量写法同样受官方支持；此处按项目使用习惯统一为命令参数。

在 Tunnel 控制台把 HTTPS 域名指向 `http://magic-cloud:5080`，与 APP_ORIGIN 一致。Docker 自动分配网络地址；后端通过 `TRUSTED_PROXY_HOST=cloudflared` 解析隧道容器地址，仅在 TCP 对端匹配时采信客户端 IP 头。公网使用 HTTPS 域名；配置 `LAN_BIND_ADDRESS` 和 `LAN_ORIGIN` 后，内网可通过树莓派 IP:5080 直接访问，断网时该入口仍可使用。

502/503 可能意味着磁盘或后端不可用，不代表密码错误。16 MiB 上传分片仍可能受具体账户/WAF/网络策略限制。公网 10 GB 下载、公开大文件及视频分发必须核实当前 Cloudflare 条款和实测，不能仅凭后端支持 Range 作保证。
