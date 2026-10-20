# 验证记录

## 0.3.0 Markdown 即时编辑验收

2026-10-20，Windows x64 / Rust 1.98.0 / GPUI Kit 0.7.3：

- `cargo test --workspace --all-features --locked`：107 项通过（核心 53、桌面 40、同步 14）。新增覆盖无损 Markdown 分块、Unicode / CRLF、嵌套列表 / 围栏 / 表格 / 引用定义、格式化选区、撤销预算与预览限制。
- `cargo fmt --all -- --check` 和全目标、全特性 Clippy `-D warnings` 通过。
- 原生窗口回归通过，证据目录 `.scratch/live-editor-28a9f45089be4730bda23dee417ba882`：实际键盘输入、点击排版段落、即时粗体预览、格式按钮、跨模式撤销重做、CRLF 模式切换不变、文末引用定义、输入法组合文字直接保存和切换后续段落、长文档源码回退与缩短后恢复、草稿恢复、历史版本以及原有学习 / 备份 / 文件夹同步流程。
- 用户正在使用的 `target/debug/studyapp.exe` 保持运行；最终原生测试通过 `cargo rustc -p studyapp-desktop --features ui-test --bin studyapp --locked -- -o .scratch/studyapp-live-test.exe` 生成独立程序。`-o` 与 Cargo 输出目录有关的提示已保留，未绕过编译错误或测试断言。
- 先复现旧编辑器需要切换预览、输入法可见文字未落入保存、组合输入后点击后续段落定位错误，以及局部解析无法识别文末引用定义的问题，再实现修复并观察回归通过。独立审查复核后无剩余 P1/P2。
- 人工检查 [编辑器截图](images/note-editor.png)：工具栏、排版段落、当前输入区域与实时效果均可见。产品使用原生段落即时编辑，并不宣称完整复刻 Typora 富文本交互。
- 本次无新增第三方依赖；版本号同步为 0.3.0，原有 Windows/Linux CI、安全检查和发布门禁继续执行。远端执行状态以对应 GitHub Actions 和 Release 为准，本节记录实际完成的本地验证。

预览采用 64 KiB / 约 400 行上限，即时块布局最多 256 KiB / 500 块，超大单块也进入源码模式。原文仍可完整编辑与保存；会话撤销最多 100 个快照、合计 4 MiB，超过预算的正文不提供会话撤销。Markdown 资源和活动内容继续过滤，已有版本保存与同步冲突机制未改变。

## 0.2.x 本地验收

2026-10-19，Windows x64 / Rust 1.98.0 / GPUI Kit 0.7.3，使用提交的 Cargo.lock：

- `cargo test --workspace --all-features --locked`：95 项通过（核心 53、桌面 28、同步 14），无失败。覆盖两个独立资料库互传、双边冲突、下载副本离线归属、配置失败及副本编辑后的幂等重试、并发修改保护、跨库旧请求拒绝、1 万条同步基线重启、DPAPI 与回环 HTTP WebDAV 条件写入。
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`、格式检查通过。
- 最终原生窗口冒烟通过：五页渲染、复习完成/撤销、计时暂停、设置保存、备份恢复、学习版本选择；真实平台文件拖放、新建/编辑/预览 Markdown、草稿恢复、版本累积，以及通过设置页测试连接和同步到本地目录。实际目标 Markdown 文件和 UI 成功状态均断言通过。证据目录 `.scratch/smoke-v020-final-e5f1be59266f4b12904068e961865ac9`，截图包含 `note-editor.png`、`sync-settings.png`。
- `cargo-audit 0.22.2`：0 个已知漏洞，8 个上游停止维护提示，未忽略公告。使用官方 RustSec 公告提交 `999ec89d5d3f10f20be14750ec8c2d8b44e0ac9b`，本机采用已下载公告 `--no-fetch`。
- `cargo-deny 0.20.2 --locked check licenses bans sources` 全部通过。新增 TLS 依赖所用 ISC 许可证已加入允许列表，发布包保留依赖声明及许可证文件。
- 独立代码审查发现的文件覆盖竞态、硬链接不支持时的阻塞、远端版本错误学习归属、配置容量与重试、重复同步、未保存目标、跨库异步请求问题均已修复并复核。保留文件夹第三方写入和真实服务商兼容性边界。

### 发布流程与范围

0.2.0 的 [main CI](https://github.com/xwwpy/StudyApp/actions/runs/39269117051) 发现 Linux 文件阻塞用例在写意图前即拒绝 `NotADirectory`，原测试却假设已写入日志。0.2.1 仅修正该测试的两种安全失败路径断言，并升级版本号；生产恢复行为未因测试而放宽。0.2.0 标签保留，失败的发布门禁未发布该版本。

0.2.0 优化构建已在本机实际打包并解压启动验证，ZIP 13,816,419 字节，SHA256 `627e06e209e0368e6bb3e2d347343aaae66ff05e274ac6db7454f6f1828e7887`。补丁版本的最终资产以对应成功 Release 为准。

0.2.1 的 Linux 核心 53 项测试全部通过，随后同步测试服务器在客户端依据 `Content-Length` 拒绝超大响应时收到 `BrokenPipe`，服务端 fixture 的无条件 unwrap 导致测试失败。0.2.2 只对刻意超限的 XML/正文响应容忍连接已关闭错误，普通响应和其他传输错误仍失败；客户端限制及生产代码未修改。0.2.2 本地全量 95 项测试与 Clippy 再次通过，最终门禁继续验证两平台及原生窗口。

2026-10-19：工作区已升级至 0.2.0，并新增 `studyapp-sync`。0.2.0 的功能范围包含托管 Markdown、草稿、拖入及 WebDAV / 文件夹同步；下方 0.1.0 结果只记录旧版本，不能作为新增同步依赖、整套窗口流程或真实存储服务的验收证据。

- 笔记 UI 已通过集成编译，4 项草稿测试通过，覆盖恢复、关闭前刷新、资料库隔离、清空顺序与损坏文件保留。命令：`cargo test -p studyapp-desktop --bin studyapp drafts::tests --offline`。
- CI 配置已要求 Windows / Linux 同时运行 `studyapp-core` 与 `studyapp-sync` 测试。Windows 工作区全特性测试、Clippy 和原生窗口冒烟仍保留。Release 同样以两平台业务测试为前置条件。
- 发布脚本验证通过：PowerShell AST 语法检查；用现有 0.2.0 调试 EXE 在全新 `.scratch/package-v020-validation-9efb3281bf0d433495906f9cbe4a9cf5` 目录打包成功。ZIP 935 个条目包含同步指南、发布说明、锁文件及 reqwest / quick-xml 通知，未包含资料库、草稿或凭据；SHA256 一致，重复输出和工作区版本不匹配均在写入前被拒绝。这项检查验证打包脚本，不作为最终 release 二进制验收。
- 三份 workflow YAML 解析通过，结构断言确认 Windows / Linux core+sync 矩阵、Windows 全特性测试 / Clippy / 原生 smoke、Release 前置依赖和全部 Actions 完整提交固定；`git diff --check` 通过。未在本地执行 GitHub Actions 或 Linux job。
- 上述本地全量测试、增强原生冒烟与审计已实际执行；GitHub 对应提交的 CI/CD 结果以 Actions 和 Release 为准。
- 自动化使用本地目录和回环 WebDAV fixture；未声称已用真实用户账号验证服务商 WebDAV、真实 NAS、微信登录、代码签名或自动更新。

---

## 0.1.0 历史验证记录

验证日期：2026-10-19。Windows x64，Rust 1.98.0，GPUI Kit 0.7.3，使用提交的 Cargo.lock。

### 本地执行

- `cargo fmt --all -- --check`：通过。
- `cargo test --workspace --all-features --locked --offline`：45 项通过（核心 35、呈现 4、工作线程 4、冒烟目录保护 2），无失败。
- `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings`：通过。
- `cargo build -p studyapp-desktop --features ui-test --locked --offline`：通过。
- 原生窗口冒烟：五个页面截图成功；实际控件点击验证导航、复习完成、撤销、暂停、设置保存；备份恢复后设置和编辑状态刷新断言通过。学习保存测试确认缺少版本时保留表单并报错，选定版本后成功创建记录和复习计划。隔离测试目录保留在本机 `.scratch`，不提交数据库。
- 人工检查原生页面截图：中文排版、Markdown 内容、复习卡片、统计图表和设置可正常显示。首页示例见 [dashboard.png](images/dashboard.png)。
- 核心回归覆盖：未回答超时、暂停/休眠/重启/时钟回退、时间段拆分、复习幂等/撤销、变更去重/防抖/离线归属、失联重连、损坏备份拒绝、事务失败回滚、跨日统计。呈现测试覆盖夏令时 23 小时自然日和不可信 Markdown。
- 工作线程测试覆盖同库互斥、保存退出、恢复成功/失败、切换成功/失败。
- `cargo-audit 0.22.2`：0 个已知安全漏洞；8 个上游停止维护提示保留，详见 README。RustSec 公告提交 `999ec89d5d3f10f20be14750ec8c2d8b44e0ac9b`。本机直接抓取公告受 Git TLS 环境影响，使用 Git 下载官方公告后 `audit --db .scratch/advisory-db --no-fetch`，未忽略任何公告。
- `cargo-deny 0.20.2 --locked check licenses bans sources`：三项通过；框架多版本依赖提示保留。
- PowerShell 打包：真实 EXE 打包成功，ZIP 内容和 SHA256 校验一致，重复输出拒绝覆盖，非法版本参数被拒绝；不包含用户资料库。

### 远端执行

最终提交 `faa9279` 的 [main CI](https://github.com/xwwpy/StudyApp/actions/runs/39262465983) 已全部通过，包括 Windows/Linux 核心测试和 Windows 桌面原生窗口测试。[v0.1.0 发布流水线](https://github.com/xwwpy/StudyApp/actions/runs/39262466051) 的构建、审计、原生冒烟、打包与发布均成功，[Release](https://github.com/xwwpy/StudyApp/releases/tag/v0.1.0) 已发布。

### 当时范围

0.1.0 交付为 Windows 本地版本。微信登录、云端保存、双端同步保留接口与设置入口，尚未接入后端；没有自动更新和代码签名。系统通知还受 Windows 通知权限影响。文件历史为运行时轮询快照，不能还原关闭应用期间的每次编辑。
