# 实际验证记录

验证环境：Linux ARM64 / Raspberry Pi 宿主机，约 8 GiB RAM。Rust/Node 均通过 Docker 执行，无宿主机工具链依赖。浏览器使用隔离的 Chromium 配置、本地 HTTPS 代理和可丢弃数据；没有使用真实 USB、Tunnel Token 或用户文件。

| 检查 | 命令/方式 | 结果 |
|---|---|---|
| Rust 格式 | 容器中 `cargo fmt --check` | 通过 |
| Rust 单元/集成 | 容器中 `cargo test --locked` | 18 项通过 |
| Rust 静态检查 | `cargo clippy --all-targets --locked -- -D warnings` | 通过 |
| Vue/TypeScript 生产构建 | 容器中 `npm run build` | 通过 |
| 全局传输管理器 | 容器中 `npm test` | 3 项通过 |
| 前端依赖审计 | `npm audit --audit-level=moderate` | 0 项已知漏洞 |
| 宿主机保护脚本 | `python3 -m unittest discover -s tests -p 'test_*.py'` | 8 项通过，使用挂载/Docker 命令替身，无真实磁盘变更 |
| Bash 语法 | `bash -n deploy/scripts/storage.sh deploy/scripts/check.sh` | 通过 |
| Compose 配置 | `docker compose --env-file .env.example config --quiet` | 通过 |
| ARM64 生产镜像 | `docker build -t magic-cloud:validation .` | 本机多阶段构建通过 |
| 浏览器功能 | Chromium + Playwright，HTTPS、真实 Rust API | 登录、新建目录、真实 Worker/分片上传、17 MiB 多分片上传在切换设置路由后继续、代码转义预览、公开分享、视频静态封面、语言/主题和手机布局通过，无浏览器运行错误 |
| 非 ext4 启动保护 | 对隔离 tmpfs 测试目录启动容器 | 按预期拒绝，报告 ext4_required |

额外验证：最终生产镜像完成 PNG 上传及 FFmpeg JPEG 缩略图生成；视频详情无播放器。停机后以只读数据绑定执行 `--diagnose`，结果 integrity=ok、identity_mismatches=0、pending_operations=0。

浏览器测试发现并修复了 SPA 深层链接的 HTTP 状态问题，已加入 Rust 回归测试。断点恢复、覆盖中断、身份隔离等由自动化集成测试验证；不将模拟测试视为真实拔盘/断电验收。

尚未验证：真实 USB 拔插/重挂自动恢复、真实断电、10 GB 文件的完整传输压力、Cloudflare 实际域名上的大文件下载及适用条款。需按 acceptance.md 在专用测试盘和实际 Tunnel 环境中执行。当前没有填入生产凭据、修改宿主机挂载/systemd 配置或启动生产 Tunnel。
