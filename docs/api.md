# REST API v1

所有响应禁止公开缓存。请求与响应通常为 JSON；分片是二进制请求体，下载/缩略图是流。时间为 UTC Unix 秒，大小为字节。所有者 API 在公网使用 `mc_session` Cookie、在显式配置的局域网入口使用 `mc_lan_session` Cookie；所有写请求必须携带与对应入口 `APP_ORIGIN` 或 `LAN_ORIGIN` 相同的 `Origin`，登录外还需要 `X-CSRF-Token`（由登录或 session 返回）。不接受前端磁盘路径。

## 接口表

| 方法 | 路径（以 `/api` 开头） | 鉴权/输入 | 成功响应 |
|---|---|---|---|
| POST | `/auth/login` | 无会话；`{password}` | `{csrf}`，设置安全 Cookie |
| GET | `/auth/session` | 会话 | `{csrf,expires_in}` |
| POST | `/auth/logout` | 会话+CSRF | `{ok:true}`，立即撤销 |
| GET | `/nodes` | 会话；`parent=root,q,sort,desc,recent` | `{nodes,breadcrumbs}` |
| POST | `/nodes` | 会话+CSRF；`{parent,name}` | 新建目录节点 |
| POST | `/nodes/batch-move` | 会话+CSRF；`{ids,parent}`，最多 500 项 | 批量移动，自动重命名同名项目，保留现有文件；`{count}` |
| POST | `/nodes/batch-trash` | 会话+CSRF；`{ids}`，最多 500 项 | 统一校验、合并嵌套选择并移入回收站，`{count}` |
| GET | `/nodes/{id}` | 会话 | 节点详情 |
| PATCH | `/nodes/{id}` | 会话+CSRF；`{parent,name,strategy}` | 保留 ID 的新节点信息 |
| DELETE | `/nodes/{id}` | 会话+CSRF | 进入回收站，`{ok:true}` |
| GET | `/nodes/{id}/download` | 会话；可选 `inline=true` 与 Range 头 | 200/206 文件流 |
| GET | `/nodes/{id}/text` | 会话 | `{text}`，UTF-8，≤2 MiB |
| GET | `/nodes/{id}/thumbnail` | 会话 | 就绪返回 JPEG；生成中返回 202 `{state:"pending"}` 并入队；失败返回明确错误 |
| GET | `/uploads` | 会话 | 未过期任务数组 |
| POST | `/uploads` | 会话+CSRF；见下文 | `{upload,chunk_size,chunks}` |
| GET | `/uploads/{id}` | 会话 | `{upload,chunk_size,chunks}` |
| PUT | `/uploads/{id}/chunks/{index}` | 会话+CSRF；二进制、`X-Chunk-SHA256` | `{index,size}`，幂等 |
| PATCH | `/uploads/{id}/strategy` | 会话+CSRF；`{strategy}` | `{ok:true}` |
| POST | `/uploads/{id}/complete` | 会话+CSRF | 最终节点，重复请求返回同一节点 |
| DELETE | `/uploads/{id}` | 会话+CSRF | 取消并释放临时文件 |
| GET | `/trash` | 会话 | 回收站根对象数组 |
| POST | `/trash/{id}/restore` | 会话+CSRF；`{parent?,strategy}` | 恢复节点 |
| DELETE | `/trash/{id}` | 会话+CSRF | 永久清理该回收站项 |
| DELETE | `/trash` | 会话+CSRF | 清空回收站 |
| GET | `/shares` | 会话 | 分享数组及所有者可复制的相对 URL |
| POST | `/shares` | 会话+CSRF；`{root,expires:null或UTC秒}` | `{id,token,url,expires}` |
| PATCH | `/shares/{id}` | 会话+CSRF；`{expires}` | 更新有效期 |
| DELETE | `/shares/{id}` | 会话+CSRF | 撤销，`{ok:true}` |
| DELETE | `/shares/{id}/record` | 会话+CSRF；必须已撤销 | 永久删除分享记录，保留原文件；未撤销返回 409，记录不存在返回 404 |
| GET | `/public/{token}` | 分享 Token；可选 `node` | `{root,node,nodes}`，无真实路径 |
| GET | `/public/{token}/nodes/{id}/download` | 分享 Token+当前子树归属 | 逐文件下载流 |
| GET | `/settings` | 会话 | storage/system/limits |
| GET | `/settings/audit` | 会话 | 最近最多 50 条登录记录；启动及每次写入时清理超额记录 |
| GET | `/settings/backups` | 会话 | 备份文件名数组 |
| POST | `/settings/backups` | 会话+CSRF | `{name}`，新的一致性备份 |
| GET | `/settings/backups/{name}` | 会话 | 数据库备份流 |
| DELETE | `/settings/backups/{name}` | 会话+CSRF | 永久删除指定备份，不影响当前数据库及原文件，`{ok:true}` |

`/health/live` 和 `/health/ready` 不带 `/api`，只返回通用状态，用于容器内部健康检查。健康端点不包含配置、文件或设备详情。

## 上传示例

```json
{
  "parent": "root",
  "name": "photos.zip",
  "size": 42000000,
  "digest": "完整文件的小写64位SHA256十六进制摘要",
  "strategy": "ask"
}
```

每片大小固定 16777216 字节，最后一片为余数；空文件无分片，直接完成。序号从零开始，任务有效期为创建后 24 小时。前端暂停通过停止新请求及中断正在传输的请求实现；继续先查询确认片。取消遇到在途请求返回 409，客户端等待请求停止后重试。

## 错误与冲突

```json
{"error":{"code":"name_conflict","details":{"choices":["overwrite","rename","cancel"]}}}
```

默认 `strategy=ask`。冲突时 HTTP 409；用户选择后以 `overwrite` 或 `rename` 重试，取消则不执行请求。自动重命名采用 `name (1).ext`。覆盖行为适用于整个冲突对象；旧对象及子树先进入回收站。

常见状态：400 参数/摘要错误；401 会话无效；403 Origin/CSRF 不符；404 对象或分享不可用；409 同名/在途冲突；416 Range 无效；429 并发/限流；503 存储或数据库错误；507 空间不足。未知输入和解析错误统一返回 `invalid_request`。`X-Request-ID` 可用于关联服务端错误日志，日志不记录密码、Cookie、分享 URL 或 Token。

浏览列表的 `q` 是全盘递归文件名搜索；`sort=name|size|type|mtime`，`desc=true|false`；`recent=true` 返回最近修改的最多 100 个文件。预览的 HTML/代码始终作为转义文本展示，视频下载不接受 inline 播放模式。

公开下载链接不可用时，浏览器导航（Accept 包含 text/html）会临时重定向至对应分享页，由页面展示过期/撤销/文件不可用的友好提示。API 客户端继续收到结构化 JSON 错误及原有状态码；网络或服务器错误在页面中单独显示连接异常，不误报为链接失效。

## 文件信息与展示

节点 JSON 的 `uploaded_at` 是可空的 Unix 秒时间戳，表示由服务器记录的上传完成时间，与文件修改时间 `mtime` 分开。记录中未提供上传时间时返回 null；移动、重命名和恢复保留原值。公开节点同样提供该字段，不暴露内部物理路径。

访客界面仅调用分享元数据和下载接口，不嵌入文件或请求缩略图。所有者界面仅对受支持的媒体与文本尝试预览；PDF、Office 等格式展示统一提示与下载入口，不发起文本解码请求。

分享的 `expires` 为未来的 Unix 秒时间戳或 null（永久有效），0 不是永久有效值。界面使用固定时长选项，确认时计算绝对到期时间。
