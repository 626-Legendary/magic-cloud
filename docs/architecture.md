# 架构与数据模型

浏览器 → Cloudflare HTTPS/Tunnel → Axum（同源 Vue 静态文件与 `/api`）→ USB ext4。两个容器共享专用 Docker 网络，不发布宿主机端口。后端单实例，SQLite 写入与文件元数据变更通过阻塞工作线程上的互斥核心串行执行；分片网络接收和下载在 Tokio 中流式处理。

## 存储

```text
USB mount/
├── files/                     原始文件夹、原始文件名
└── .magic-cloud/
    ├── storage-id             明确初始化时生成的磁盘标记
    ├── initialized            已初始化数据库的持久标记
    ├── app.lock               单实例锁
    ├── database/magic-cloud.db SQLite 与 WAL/SHM
    ├── uploads/               按任务 ID 命名的临时文件
    ├── trash/                 按被删除根对象 ID 保存的子树
    ├── thumbnails/            文件身份关联缓存
    ├── operations/            尚未发布的新目录
    └── backups/               一致性 SQLite 快照
```

`nodes` 保存稳定 ID、parent、name、path、kind、size、mtime/mtime_ns、dev、ino、deleted；根 ID 为 `root`。路径不承担公开身份。`shares` 引用根 ID，保存 Token 摘要、使用 SESSION_SECRET 派生密钥加密的 Token、期限和撤销状态；加密副本仅供所有者重新复制链接。更改 SESSION_SECRET 后旧分享仍可验证摘要，但所有者无法解密复制，应重新生成分享。

`sessions` 保存随机会话摘要、CSRF Token、到期时间和密码/密钥配置世代。`login_audit` 保留最近 1,000 条记录。`uploads` 保存文件摘要、目标 ID、大小、冲突策略、状态、期限、最终节点；`chunks` 唯一键为上传 ID/片序号。`trash` 保存被删除子树原位置及实际存放路径。`operations` 保存跨 SQLite/ext4 的持久变更意图。`config` 绑定数据库与 storage-id。

迁移位于 `backend/migrations`，事务应用，`PRAGMA user_version` 标识版本。未知更高版本拒绝启动。SQLite 开启 WAL、synchronous=FULL、foreign_keys、5 秒 busy_timeout，并使用依赖锁文件固定 bundled SQLite。运行数据不随镜像升级覆盖。

## 可恢复文件操作

1. 校验节点身份、目标目录、冲突及子树边界。
2. 在数据库提交操作意图（有序物理重命名、最终节点、回收站与分享变化）。
3. 使用目录句柄及 `renameat2(RENAME_NOREPLACE)` 逐项执行，fsync 两侧目录。
4. 在一个数据库事务中提交全部元数据并删除操作意图。

覆盖先将旧对象移入回收站，再将新对象发布。重启时按 inode/device 检查每个动作的来源或目的地；已完成动作跳过，未完成动作前滚。来源和目标均异常时停止启动，不能猜测或覆盖。已确认完成但尚未提交数据库的多步覆盖同样可恢复。目录创建使用私有暂存目录后发布，未引用的暂存目录不会暴露给用户。

永久清理采用“先删除已隔离内容，再删除回收站记录”；中断时重复清理是幂等的。保留删除节点墓碑，防止旧分享重新关联新对象。

## 上传

创建任务时预留剩余需求及安全余量；临时文件在与最终文件相同的文件系统。每片按固定偏移写入，边接收边算 SHA-256，长度/摘要正确并 fsync 后才记录确认。相同片重传核验请求内容，不覆盖已确认分片；不同摘要返回冲突。未确认的部分写入由后续重试覆盖。完成时在发布前核验分片数、实际长度与完整 SHA-256。

前端 Worker 每次以 4 MiB 读取计算完整文件摘要，分片校验最多占用两个 16 MiB 缓冲/文件；不会将整个 10 GB 文件载入内存。全局完成回调驱动队列，有限失败退避。页面关闭后由服务端任务恢复，绝不保存用户文件字节到 localStorage。

## 后台任务

每分钟清理过期上传/回收站并检查备份调度；备份使用 SQLite Backup API。启动后后台逐个处理媒体缩略图，按需请求也使用同一并发信号量。存储 watchdog 每两秒检测绑定设备与标记，I/O 异常会令核心拒绝后续请求并退出进程。宿主机 timer 每五秒检查挂载状态并负责重新创建容器。
