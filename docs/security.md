# 安全边界

信任 Pi 管理员、Docker 管理员和该专用 USB 目录的操作系统所有者。不向公网发布 Docker 或管理 socket。不承诺防御已控制宿主机、同 UID 进程或内核的攻击者。浏览器输入、文件内容、公开分享访问者均不可信。

- Linux `openat2`（内核 ≥5.6）以 USB 根目录句柄为基准，要求 BENEATH、NO_SYMLINKS、NO_XDEV；不支持时安全失败。物理重命名使用已打开的父目录和 NOREPLACE，禁止目录移入自身后代。
- 下载、文本和缩略图从经过元数据核验的同一个已打开文件句柄读取，拒绝设备/inode/文件大小/修改时间异常。外部直接写盘不属于支持场景；元数据身份校验不是对本机恶意管理员的完整内容防篡改方案。
- `.magic-cloud` 不由静态服务提供，也不在节点、搜索或公开分享接口中枚举。真实文件名不拼接到 shell 命令中。
- 密码仅来自环境，启动时使用随机盐 Argon2id 派生验证值；数据库不存明文密码。会话 Token 为 256 位随机值，只存 SHA-256。会话绝对有效期七天，配置世代更换、退出及到期均使旧会话无效。
- 写请求严格检查 Origin；除登录外要求会话关联 CSRF Token。公网 Cookie 为 HttpOnly、Secure、SameSite=Strict、Path=/；显式配置的 LAN_ORIGIN 使用独立 mc_lan_session Cookie，不含 Secure，仅供 HTTP 局域网使用。两种会话采用不同的哈希命名空间，不能跨入口复用；Cookie 不设置 Domain，写操作同时校验 Host 对应的精确 Origin 和 CSRF。登录每来源十分钟最多十次，所有来源合计最多五十次，减少 Argon2 CPU 滥用；应用重启会清空内存限流窗口。
- 每次登录通过 Docker DNS 解析 `TRUSTED_PROXY_HOST`（默认 cloudflared），仅当 TCP 对端匹配解析地址时采用 `CF-Connecting-IP`。其他来源、DNS 失败或非法客户端 IP 均回退到 TCP 对端地址；不信任整个 Docker 网段。
- 分享通过摘要检索；所有者重新复制链接使用 ChaCha20-Poly1305 加密副本。公开接口逐次校验有效期、撤销、对象身份与当前祖先链，不按客户端路径读取。
- 响应带 private/no-store、nosniff、no-referrer 和 CSP；代码文本由 Vue 转义。禁止在 Cloudflare 自定义缓存规则中覆盖这些要求。分享链接在 URL 中属于访问凭据，收到链接的人都可下载，不附加密码。
- FFmpeg 独立子进程、固定参数和 demuxer，通过固定 `/proc/self/fd/0` 读取核验后的文件句柄，保留 seek 能力。仅启用所需 file/pipe 协议，不开放网络协议，MOV 显式禁用外部轨道与绝对路径引用。限制线程、地址空间、CPU、执行时间和输出大小，单 worker 顺序处理并去重；发布封面前再次确认对象未删除且身份一致，避免删除与解码并发时重新留下缓存。失败显示明确状态。运行镜像非 root，不提供 Docker Socket；Compose 使用 Docker 默认的文件系统、capabilities、安全选项、资源和日志设置。
- 上传大小、片序号、片长度、SHA-256、2×2 并发和空间预留由服务端再次检查。未完成文件只存在私有临时区；完整校验通过前不可下载或分享。

数据库备份包含会话及分享元数据，应与原始文件一样保密。SESSION_SECRET 必须独立保存并备份；复制数据库不能替代原始文件备份。日志仅输出通用错误类别、请求 ID 和状态，不记录请求体/URI/会话/Token。

局域网入口仅接受显式配置的私有/回环 IP 的 HTTP Origin（含端口），不根据任意 Host、Origin 或转发头自动授权。该入口传输不加密，仅供可信局域网使用；公网 HTTPS 会话保持原安全属性。前端对普通 HTTP 使用 getRandomValues、WASM SHA-256 和选区复制回退，不要求关闭浏览器安全检查。
