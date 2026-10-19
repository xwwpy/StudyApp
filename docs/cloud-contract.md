# 自有存储同步与账户扩展边界

## 0.2 已实现的笔记文件同步

`crates/studyapp-sync` 提供独立 `Provider` 接口、本地文件夹实现、HTTPS WebDAV 实现和三方同步规划器。桌面在独立后台线程执行网络操作，并在应用结果前通过核心 CAS 再次检查本地版本。设置支持关闭、一个文件夹 / NAS 目标或一个 WebDAV 目标，以及手动、可选定时同步。

同步对象限于 `StudyApp/notes/<UUID>.md` 的最新 UTF-8 Markdown。不传播删除，不上传学习记录、复习事件或完整快照历史；这些仍由本地 SQLite 和显式备份管理。导入的外部原文件不能被拉回结果覆盖。并发修改先保存独立冲突副本；失败项不推进同步基线。

WebDAV 生产设置仅接受 HTTPS，校验证书，禁止重定向；覆盖已有对象依赖服务端强 ETag 与 `If-Match`，新建依赖 `If-None-Match: *`。本地目录使用操作系统文件锁、硬链接 create-only 和哈希复查；NAS 必须支持相应操作，第三方不参与锁的竞争无法由普通文件系统提供原子比较与替换保证。协议、限制和迁移步骤见 [同步指南](sync-guide.md)。

WebDAV 密码使用当前 Windows 用户 DPAPI 加密，受保护字节与非敏感配置存于资料库旁 `sync-config.json`，不写入 SQLite 或备份。UI 不提供 HTTP 或关闭 TLS 校验的选项；回环 HTTP 仅用于显式测试构造器。

## 尚未实现的微信与账户级云服务

`crates/studyapp-core/src/integration.rs` 保留未来账户接口。`UnavailableServices` 对认证和旧的账户同步操作仍返回明确错误；它不同于 0.2 独立的笔记文件同步实现。`StorageMode::{Local, Cloud, Hybrid}` 是未来账户存储模型，不代表现有产品提供只云端资料库。

`AuthProvider` 包含 `begin_wechat_authorization`、`poll_authorization`、`restore_session`、`sign_out`。授权会话模型只有账号 ID 与过期时间，没有真实令牌和微信后端。旧 `SyncProvider::synchronize(mode)` 也不是 WebDAV Provider API，尚无账户隔离、全量实体同步协议或部署服务。

本地实体的稳定 ID、UTC 时间、实体修订和独立快照/事件 ID 可用于未来设计，但不构成已完成的云端删除标记、幂等键或全量同步元数据。

## 接入账户服务前的要求

1. 注册微信应用并部署受信后端，由后端保管 AppSecret；使用一次性 state、防重放授权 ID、过期及取消，桌面轮询授权状态并接收自己的短期会话。AppSecret 不能进入可执行文件。
2. 实现会话恢复、刷新、撤销和退出，令牌使用系统安全存储，非敏感配置独立保存。当前 WebDAV 密码保护不等于已实现账户会话管理。
3. 定义 TLS、账号隔离、分页、协议版本、服务端修订、幂等键、增量操作、删除语义、限制及退避重试；服务端独立验证所有数据。
4. 笔记、学习摘要与复习事件都需保留并发修改，重试不能重复创建学习或复习事件。现有笔记 Markdown 同步不覆盖这些实体。
5. 明确 Local / Cloud / Hybrid 的离线行为、缓存和持久操作队列，仅在服务端确认后显示同步成功。
6. 不同账户使用隔离资料库与缓存，迁移和退出资料处理由用户明确选择，先预览、去重和备份，并可恢复失败。

以上是未来要求，不代表已有 API endpoint、微信接入或云部署。验收还需覆盖授权过期、撤销、离线重试、并发、跨账户、备份恢复和协议迁移；代码签名与自动更新也尚未配置。
