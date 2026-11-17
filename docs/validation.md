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

## 2026-11-17 默认端口调整

默认端口已统一为 5080：Rust、Docker 镜像 ENV/EXPOSE、Compose、开发代理及文档一致；本地 `.env` 同步更新，未提交秘密。检查覆盖仓库中唯一的 Dockerfile、compose.yaml 和 .dockerignore，补充环境备份文件及测试产物的构建上下文排除规则。

- 示例配置和本地配置的 `docker compose config --quiet` 均通过。
- `docker build -t magic-cloud:local .` 在 ARM64 上构建成功，包含 Vue/TypeScript 生产构建。
- 使用隔离 Docker volume 和 `--network none` 测试镜像，主动取消 PORT 环境变量后，`http://127.0.0.1:5080/health/ready` 返回 `ok`，验证 Rust 默认端口；旧端口未监听。
- 镜像 EXPOSE 为 5080/tcp；Compose 实际端口为 5080，健康检查跟随 PORT，存储绑定禁止自动创建源目录。
- 测试容器与 volume 已清理，未启动生产 Tunnel 或修改真实 USB 数据。

## 简化 Compose 与动态代理识别

按用户要求移除两个服务的自定义网络/固定 IP、read_only、cap_drop、security_opt、内存/CPU 上限、tmpfs 和日志配置，改用 Compose 默认网络及 Docker 默认运行设置。保留 USB 安全绑定、现有 UID/GID 和健康检查。Dockerfile 继续使用与 cloud-notebook 类似的多阶段构建结构，分别构建 Vue 与 Rust。

- 示例及本地 `docker compose config --quiet` 均通过；解析结果确认仅有默认网络、无固定 IP 或已移除的运行限制。
- Rust 测试 19 项通过，包含 DNS 代理识别、非代理头部拒绝和解析失败回退；Clippy 严格检查通过。
- ARM64 `docker build -t magic-cloud:local .` 成功。
- 在临时 Docker 网络/volume 中验证：默认 DNS 识别代理；代理重建并取得不同 IP 后继续识别；其他容器占用旧 IP 后，伪造 CF-Connecting-IP 不被采信。临时容器、网络和 volume 已清理，未重建生产服务。

## 局域网 HTTP 入口

增加显式 `LAN_BIND_ADDRESS`/`LAN_ORIGIN` 配置；当前 Pi 绑定 `192.168.0.101:5080`，公网 HTTPS 入口保留。LAN 使用独立 Cookie 和会话哈希命名空间；公网 Secure Cookie 不变。写操作按入口精确校验 Origin，并继续要求 CSRF。

- Rust 格式检查、22 项测试及严格 Clippy 通过；覆盖 LAN 登录/退出/过期会话、来源与 Host 匹配、CSRF、公网/局域网会话不可互用、LAN 关闭和非法 Origin。
- Vue/TypeScript 生产构建及 5 项前端测试通过，包含普通 HTTP 环境下任务 ID 和 SHA-256 校验回退。
- 通过隔离 Chromium 在真实非安全上下文 `http://192.168.0.101:15080` 测试（未启用安全上下文绕过）：登录、刷新保持登录、真实上传/下载内容、转义预览、复制及公开分享、17 MiB 分片上传跨路由继续、退出清除 Cookie 全部通过。可重复脚本为 `tests/lan-browser.mjs`，仅对可丢弃部署运行。
- ARM64 镜像重新构建成功；确认无待完成上传后重建生产两容器，将旧固定网络迁移到 Compose 默认网络。
- 部署后 LAN 健康检查返回 ok，LAN 页面和公网 HTTPS 页面均为 HTTP 200。端口仅绑定宿主机 `192.168.0.101:5080`。临时测试容器、volume 和浏览器已清理。

## Logo 与文件界面调整

同步更新的根目录 Logo.png 到前端公共资源，启动页、登录、侧栏、空状态、公开分享及网页图标共用该资源；favicon 添加内容版本以刷新缓存。语言切换采用 Lucide Languages，文件夹上传采用 FolderUp 并放在文件上传按钮右侧。新用户默认网格，保留用户手动保存的视图选择。预览槽与缩略图使用 1:1 比例，手机布局同步调整。

- 前端生产构建与现有 5 项测试通过。
- 隔离 Chromium 验证 Logo 字节一致、三个语言图标入口、实际语言切换、默认网格及视图持久化、按钮顺序和真实文件夹上传。
- 桌面与 390px 手机视口实测缩略图宽高相等，移动端按钮顺序正确、无水平溢出；检查了截图及浏览器运行错误。
- 确认无待完成上传后仅重建应用容器，隧道保持运行。部署后公网和局域网 Logo 响应摘要均与新源文件一致，后端健康。临时测试容器、volume 和浏览器已清理。


## 品牌文案与失效分享提示

接入提供的中文 Hero、登录欢迎、首页欢迎、空文件夹、上传中/成功、空回收站、搜索无结果及网络异常文案，补全对应英文。完整产品介绍位于设置中的“关于麦吉云”。分享页新增独立的失效状态和网络异常状态，不显示原始 not_found；浏览器访问失效下载链接返回分享提示页，API JSON 行为保持兼容。

- Rust 23 项测试、严格 Clippy 通过，包含失效下载的浏览器重定向与 API 404 区分。
- 前端生产构建与 7 项测试通过，包含网络错误与用户取消的区分；网络状态保持有限重试。
- 隔离 Chromium 通过真实上传核验成功提示，检查搜索/回收站空状态、Hero 与完整产品介绍、中英文切换；实际创建并撤销、过期和删除分享对象，验证统一失效文案及旧下载链接导航。
- 模拟 503 后显示网络异常而非链接失效，重试后恢复正确失效状态。390px 手机页面无横向溢出，检查了失效页和登录页截图。
- 确认无待完成上传后仅重建应用，隧道保持运行。部署后的 LAN 与公网域名均通过真实浏览器验证失效分享和旧下载链接的友好提示，服务健康；临时测试容器、volume 与浏览器已清理。

## 首页 Hero 插画

使用用户提供的 Hero.png 作为登录首页左侧全幅插画，通过渐变遮罩保证文案可读；品牌标识使用现有 Logo。手机端将插画与下方文案衔接，保留企鹅主体。图片经 Vite 内容哈希打包并优先加载。

- 前端生产构建及现有 7 项测试通过，ARM64 Docker 镜像构建成功。
- 隔离 Chromium 检查 1440×1000 桌面、1280×720 英文、390×844 手机及 320×800 英文深色布局截图；图片正常加载，无横向溢出或运行错误，真实登录成功。
- 确认无待完成上传后仅更新应用容器。公网与局域网经浏览器验证图片响应与原始 Hero.png 摘要一致，应用健康；临时容器、volume 和浏览器已清理。
