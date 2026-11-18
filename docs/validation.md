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

`tests/visitor-details-browser.mjs` 检查访客页无文件预览请求、文件名不重复、下载入口、统一不支持预览提示、详情字段、居中上传模态框和英文登录布局。它使用模拟 API 数据，不会修改真实文件；需要 `MC_BROWSER_CDP` 与 `MC_TEST_ORIGIN`。

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
