# 验证指南

本指南说明如何验证构建、数据安全与界面行为。测试结果应由当前执行输出判断；文档不替代目标服务器上的实际验收。

## 容器化检查

在项目目录执行：

```bash
./deploy/scripts/check.sh
```

检查脚本包括存储脚本语法、Python 测试、Rust 格式/测试/Clippy、前端构建/测试/依赖审计、示例 Compose 配置及镜像构建。宿主机需要 Docker，无需安装 Node.js 或 Rust。

可按模块执行：

```bash
# 脚本语法与 Python 测试
bash -n deploy/scripts/storage.sh
python3 -m unittest discover -s tests -p 'test_*.py'

# 使用已配置的开发容器或本地工具链
cd frontend
npm ci
npm run build
npm test
```

后端应执行 `cargo fmt --check`、`cargo test --locked` 和 `cargo clippy --locked --all-targets -- -D warnings`。

## 可丢弃浏览器测试

浏览器回归会创建文件、文件夹、分享和上传任务。使用独立数据卷、专用密码及单独端口，禁止指向正式存储。

`tests/design-browser.mjs` 需要 Playwright、可访问的 Chromium CDP 地址，以及：

```bash
export MC_TEST_DISPOSABLE=1
export MC_TEST_ORIGIN=http://192.168.0.101:15080
export MC_TEST_PASSWORD=<测试密码>
export MC_BROWSER_CDP=http://127.0.0.1:19222
export MC_TEST_ARTIFACTS=/tmp/magic-cloud-browser-results
node tests/design-browser.mjs
```

请把地址换为可丢弃部署的真实入口。脚本要求初始存储为空，检查真实上传、文件分类、选择、列表尺寸、键盘菜单/模态框、设置标签、主题对比度、移动导航及减少动效。

`tests/thumbnail-lifecycle.mjs` 验证后台主动生成、缓存复用和删除清理；另需配置 `MC_TEST_STORAGE` 和小于 16 MiB 的 `MC_TEST_VIDEO`。仅挂载测试数据。测试完成后移除对应容器与数据卷。

`tests/thumbnail-transparency.mjs` 使用真实透明 PNG 上传检查生成缩略图的 Alpha 通道、中心像素、PNG 格式及浅/深主题显示，需要可丢弃部署标记、测试地址/密码和浏览器 CDP 地址。

`tests/visitor-details-browser.mjs` 比较同一文档在网格、列表、最近文件、回收站、详情、分享、上传任务和备份列表中的 SVG 与类型色，覆盖大小写后缀及浅/深主题；同时检查访客页无文件预览请求、文件名不重复、下载入口、统一不支持预览提示、详情字段、居中上传模态框和英文登录布局。它使用模拟 API 数据，不会修改真实文件；需要 `MC_BROWSER_CDP` 与 `MC_TEST_ORIGIN`。

`tests/loading-drag-browser.mjs` 通过模拟 API 延迟检查加载页尺寸，并回归拖拽取消、移出、嵌套元素、窗口失焦、事件中断及黄色安全图标。只读运行，使用浏览器 CDP 与测试地址。

## 数据库结构

项目使用 `backend/schema.sql` 这一份当前结构，不维护历史版本迁移链。独立测试存储中检查：

1. 空数据库按当前结构创建，包含独立的 `uploaded_at` 字段。
2. 新上传完成时记录服务器时间；未提供上传时间的记录显示“未记录”。
3. 重命名、移动与恢复保留节点身份和上传时间。
4. 非当前结构标记拒绝启动。
5. 数据库备份可通过完整性检查，恢复操作不混用不同状态的数据库与原始文件。

修改数据库定义前备份数据，使用可丢弃数据库验证当前结构。

## 界面验收

至少检查桌面、768px、390px 和 320px，覆盖中英文、浅/深主题：

- 所有文件类型图标与颜色正确；`.ts` 作为代码；扩展名大小写不影响分类。
- 图片/视频预览区保持方形、内缩和圆角；ICO、FLV、WMV 封面及损坏文件占位正常。
- Word/PDF 等文件显示统一提示，不请求文本或嵌入文件页面。
- 文本空文件、非法 UTF-8、超限文件、快速切换详情时不会泄露原始错误或显示旧内容。
- 详情信息与精确字节数正确，未提供的上传时间明确显示“未记录”。
- 访客单文件页只显示类型图标和下载信息，文件名最多两处，页头安全标识明确。下载权限与过期状态由服务端验证；创建/修改分享均为八项快捷有效期，0 不可输入或作为永久值。撤销前确认风险，取消不发出撤销请求。
- 上传框在桌面和手机均居中，键盘焦点受控，关闭后返回触发按钮。
- 登录文案、五项卖点 3+2 居中、按钮及 Footer 无重叠。
- 页面无横向溢出，读屏标签与状态明确。

## 服务器验收

使用 [验收清单](acceptance.md) 检查实际存储断开/恢复、重启、异常中断、大文件和数据库恢复。测试存储不能承载系统或重要数据。公网代理行为与局域网直连分别验证。

## v1.0.1 验证记录（2026-11-18）

本次使用 Raspberry Pi 5 Model B、Debian 13、ARM64、Linux 6.18。所有写入回归使用独立容器、15080/15081 测试端口和 `.scratch-smoke/v1.0.1/` 下可丢弃数据，未使用正式文件。

| 检查 | 结果 |
| --- | --- |
| `deploy/scripts/check.sh` | 通过；含以下格式、测试、静态检查、Compose 和 ARM64 镜像构建 |
| Python 部署/存储检查 | 16 项通过；含本地模式无需 UUID/挂载点、身份缺失拒绝、恢复锁止会停止 Docker 重启循环、fstab 冲突与幂等 |
| Rust | `cargo fmt --check`、`cargo check --locked`、37 项测试、`cargo clippy --locked --all-targets -- -D warnings` 通过 |
| 前端 | Vue/TypeScript 生产构建、15 项 Vitest 测试通过；npm audit 0 漏洞；项目未配置 ESLint |
| Compose | 示例配置在默认和 `--profile tunnel` 下解析通过；局域网部署无需公网 Origin/Token |
| ARM64 | 生产 Dockerfile 构建通过，并在上述 Pi 5 上实际启动；UID/GID 为 10001，健康检查通过 |
| AMD64 | 已执行 `docker build --platform linux/amd64`，因本机未配置跨架构执行环境而报 `exec /bin/sh: exec format error`；未完成镜像构建或 AMD64 运行验证 |
| 构建输入 | 完整构建上下文约 2.75 MB；排除宿主机数据、环境文件、node_modules、target、dist；Rust 编译缓存按架构隔离 |
| 浏览器 | 现有 7 个脚本均通过：browser、design-browser、visitor-details-browser、loading-drag-browser、lan-browser、thumbnail-transparency、thumbnail-lifecycle |

浏览器覆盖真实 Worker/分片上传、下载内容、文本转义、公开分享、语言/主题、导航/键盘焦点、搜索控件、回收站和缩略图。新增断言覆盖 EXE/7Z/ZIP/RAR/JSON/JS/TS/PY 等统一角标，网格、列表、详情、回收站和分享复用组件；未知类型使用 FILE。关于页面中英文及 v1.0.1 显示正确，320px 设置页无横向溢出，列表行高仍为 48px。透明 PNG 的 Alpha 和颜色校验，以及视频封面后台生成、缓存复用、失败降级和删除清理均通过。

运行层面分别验证 `local` 和 `external`：第一片上传后强制重建容器，原任务与分片保留并能续传；完整下载 SHA-256 一致；文件夹分享、数据库在线备份 `integrity_check=ok`、重启后文件/会话持久化、回收与恢复均通过。篡改测试存储 ID 后 watchdog 退出码为 74，错误身份启动退出码为 1；恢复原标记并重建后文件内容保持一致。不挂载数据目录时，本地模式以 `mountpoint_required` 拒绝启动。

本地模式安装器另在一次性 Linux 容器内执行，Docker/systemctl 使用记录命令的替身，验证其保留本地路径和 fstab、生成正确的绝对 ExecStart 并启用 timer。此项不等于已在宿主机安装系统服务。

尚未验证：物理 U 盘拔插/EIO、整机重启/断电恢复、真实 Raspberry Pi OS（本机为 Debian 13）、AMD64 主机、10 GB 全尺寸传输、真实 Cloudflare 域名及账户限制。本次 external 运行测试使用独立 ext4 目录绑定与标记异常，不能替代物理掉盘验收。宿主机 fstab/systemd 安装需要管理员密码，仍需在目标服务器执行安装器并安排重启/拔插验收。
