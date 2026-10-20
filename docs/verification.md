# 验证记录

## 0.5.0 原生编辑修复与笔记交互验收

2026-10-20，Windows x64 / Rust 1.98.0 / GPUI Kit 0.7.3 / WebView2：

- 全量 Rust **186 项测试通过**（核心 78、桌面 92、同步 16）；格式检查及全目标全特性 Clippy `-D warnings` 通过。新增设置兼容/持久化、快速交错保存、目录目标、过期动作、Enter 去重、嵌套滚轮和 flush 响应匹配回归。
- 前端类型检查、离线资源构建和 **30 项浏览器测试通过**。新增 Windows 分拆 UTF-16 输入、撤销重做、不完整字符保存失败后继续编辑、代际替换、4097 个累计补丁，以及恰好 10 MiB 下每个中间版本的字节预算检查。未修改内容的混合换行保留。200 次编辑测量中位数 1.2 ms、P95 1.9 ms，27 个渲染行、226 个 DOM 节点；该数据不是物理键盘到屏幕的端到端延迟。
- 修复前原生窗口连续采集 60 次、约 3 秒，WebView ready、焦点和命中测试均正常，但桌面正文像素仍为空白。根因是 GPUI Windows `CreateTargetForHwnd(hwnd, true)` 将不透明绘制层置于子窗口之上。随源码维护的 `gpui-pre-windows 0.3.9` 补丁改为 `false`；独立比对确认上游源码仅有该行为变化与说明，完整源码、许可证、上游哈希和修改说明进入发布包。
- 最终原生证据目录：`.scratch/v050-final-12ea9e37abba4a8e908f73aa28a87c7b`。独立 EXE 使用全新资料库，未停止用户程序。实际桌面 BMP 确认首次阅读/编辑、历史返回、菜单关闭、设置返回和缩放后正文可见；真实 Windows 键盘输入含 Emoji，保存拒绝后继续编辑，正常关闭时最后输入 `关闭窗口前的末次输入 🧠` 精确落盘。GPUI 截图与 WebView DOM 状态只作为辅助，不代替子窗口像素检查。
- 实际指针拖放到目录、右键键盘移动到未分类、复制路径、Enter 创建且不重复、路径开关在列表/详情/编辑/空间管理生效、悬停三项日期均通过。空间管理滚轮在中间及两端均不带动外层；滚动条拖到末端断言偏移等于负最大偏移、最后文件可见；列表外仍可滚动设置页。长标题不挤出操作按钮。来源文件字节保持不变。
- 原有完整原生回归继续通过，包括 1,676 行历史差异的滚动、返回编辑、版本保存、目录管理、草稿恢复/删除、取消管理、资料迁移和文件夹同步。设置默认关闭且兼容旧 JSON/数据库，重启持久化由独立核心用例验证；两个设置命令只更新各自字段，避免快速操作覆盖。
- 独立审查已关闭发现的设置覆盖竞态、悬停遮挡、Unicode 保存拒绝恢复和累计补丁上限问题，无剩余可操作问题。`cargo-deny` 许可证/来源/依赖策略通过；`cargo-audit` 官方公告库检查为 0 个已知漏洞，保留 9 项停止维护及非 Windows glib 的 unsound 警告。
- CI/CD 增加实际桌面 BMP、原生窗口状态、交互报告和日志归档；正式版本由两平台业务测试、前端检查、Windows 全量测试/Clippy/原生回归、安全和许可证检查通过后发布。

提交 `a40c0cd` 的 [main CI](https://github.com/xwwpy/StudyApp/actions/runs/39412304784) 与[安全和许可证检查](https://github.com/xwwpy/StudyApp/actions/runs/39412304696)均已成功。Windows/Linux 业务测试、编辑器检查、Windows 全量测试/Clippy 及真实原生窗口回归全部通过。

[v0.5.0 发布流水线](https://github.com/xwwpy/StudyApp/actions/runs/39412964578)全部成功，[正式 Release](https://github.com/xwwpy/StudyApp/releases/tag/v0.5.0)已发布。从 GitHub 重新下载 ZIP，14,753,422 字节，SHA256 `668f5c7ec904cac72e7471d617a15f0f2be002645631dfd5a1777a74f538e893`，同时匹配校验文件和 GitHub 资产摘要。实际包内修改后的源码、许可证和声明均已核对；解压到全新 `.scratch/published-v050-b079fd8ce32d47659b6b62d2c8454920` 后，正式 EXE 通过五页渲染和离线 WebView2 ready 检查。发布后的文档提交仅补充记录，未修改版本标签或已发布代码。

本地优化发布包为 `WW-StudyApp-0.5.0-windows-x86_64.zip`，14,752,576 字节、986 个条目，SHA256 `c2a217c5fc4513c9b87c1d05c7c4322b5096afb88285d7aaa3248b3bc17a9945`。已检查包内 EXE、修改后的 GPUI 源码/许可证/来源说明，不含数据库、草稿、同步配置或 PDB。解压到全新的 `.scratch/package-v050-175e38eb9b35482d83404a2bd4aa3374/unpacked` 后，实际 `WW-StudyApp.exe` 通过五页渲染和离线编辑器 ready 检查。此启动检查不代替 ui-test 构建的完整交互验收；GitHub 发布包由 CI 独立构建。

应用显示名称为 **WW-StudyApp**，现有数据目录、同步协议和凭据标识保持兼容。仍需 WebView2 Runtime；真实硬件输入法与各厂商 WebDAV/NAS 没有全矩阵验证，微信和账户级全量云保存仍预留。同步仅传输最新 Markdown；暂无代码签名和自动更新器。

## 0.4.0 连续编辑、目录和资料迁移验收

2026-10-20，Windows x64 / Rust 1.98.0 / GPUI Kit 0.7.3 / WebView2：

- 格式检查、全目标全特性 Clippy `-D warnings` 通过；`cargo test --locked --workspace --all-features` **165 项通过**（核心 70、桌面 79、同步 16）。
- 前端类型检查与离线资源可重复构建通过。23 个浏览器回归覆盖单编辑面、跨段键盘选区/撤销、所有语言高亮、任务键盘勾选、表格、恶意 HTML/链接/图片、CDP 输入法组合与 flush、过期 flush、源码模式、超限粘贴、4096 patch 上限、混合 EOL 精确字节预算、生产环境无 test API。
- 原生窗口测试使用独立 EXE，未停止用户正在运行的旧程序。证据目录 `.scratch/continuous-v040-8e1c2574ef3a4721993690090f76abc7`：真实 WebView IPC、跨段格式/撤销、CRLF 保留、桥接错误恢复、旧代际错误忽略、只读恢复仍只读、版本保存、草稿恢复/删除、取消管理/重新导入、文件夹同步，以及正常关闭前的最后输入落盘通过。五页 GPUI 截图与单独 WebView 截图保留；GPUI GPU 截图本身不包含子 HWND。
- 存储与目录扩展的最终原生证据：`.scratch/library-v040-67c714e7892448379d7ca03ebdccd84d`。1000×700 窗口、200 字标题、1,676 个差异显示行：实际滚轮使偏移改变，滚到末尾后最后一行可见，差异视口 267.2×209.6 逻辑像素；返回编辑后 WebView2 `Input.insertText` 的真实 DOM 输入通过 IPC 保存到文件。目录筛选/移动、删除新草稿所属空目录后的未分类回退、维护期间保存互斥、逐份确认清理、源文件字节不变、迁移并恢复已落盘草稿、再切回原库及关闭前末次输入均通过。
- 9 项核心目录测试覆盖 8 层限制、循环、同级重名、源文件不移动及备份校验；21 项存储/worker 测试覆盖位置发布失败、重启位置、显式 `--data-dir` 隔离、Windows junction、外部文件和旧库路径拒删、删除失败重试及同步中的互斥。独立审查未留下已知 P0/P1。
- 新原生界面在 Windows 默认 1 MiB 调试主线程栈下复现栈溢出；GUI EXE 现明确预留 8 MiB、按需提交，随后完整原生回归通过。测试还发现并修复长标题挤出目录移动目标，以及历史列表边界；CI 保留窗口截图和运行日志。
- 同步回归覆盖已取消管理 UUID 不上传/不远端复活、迟到冲突不建副本，以及在途同步时取消管理/恢复/切库拒绝。草稿真实文件锁测试覆盖 Persist/Discard 失败不冒成功、不丢原字节。schema 1/2 到 3 及备份恢复均覆盖。
- 性能测量：417,780 字节文档的浏览器 200 次编辑，中位数 1.2 ms、P95 1.8 ms，27 个渲染行、226 个 DOM 节点；Rust 371,600 字节文档的 200 次变更处理中位数 0.61 ms、P95 1.11 ms。前者包含事务/装饰/DOM/强制布局，后者含原文补丁和行索引；这不是物理键盘到屏幕的端到端延迟，也不代表所有硬件或所有 Markdown 输入。
- `cargo-audit 0.22.2` 使用更新后的官方 RustSec 公告库（`999ec89d5d3f10f20be14750ec8c2d8b44e0ac9b`）：0 个已知漏洞，9 项停止维护警告和 glib VariantStrIter 的 unsound 警告保留；glib 来自 Wry 的非 Windows 平台依赖，不在发布的 Windows 编译目标中。未添加忽略规则。`cargo-deny 0.20.2` 的许可证、依赖策略、来源检查通过。
- npm 官方安全公告审计：0 个已知漏洞。锁文件使用官方 registry 来源与完整性哈希；37 份实际打包依赖许可证由构建校验并纳入打包。全新目录从官方 registry 安装后，类型检查和离线资源字节一致性再次通过。
- 独立审查发现并修复：无落盘 ACK 的关闭/切库、草稿删除失败的 owner 丢失、过期结果错误解除输入冻结、只读恢复解锁、JSON 转义导致 IPC 容量不足、旧代际错误影响新文档、跨库同步绕过取消管理，以及前端嵌套格式/多行链接/表格边界问题。

远端首轮 CI 的 Windows 全量测试暴露旧 WebDAV fixture 的非阻塞 socket 继承差异：服务器在请求字节尚未到达时 `read_exact` 得到 `WouldBlock`。新增延迟请求用例在本机先复现失败；接受连接后显式恢复阻塞模式并保留读写超时，16 项同步测试和 165 项全量测试均通过。生产同步逻辑未改变。

提交 `7ac3856` 的 [main CI](https://github.com/xwwpy/StudyApp/actions/runs/39390827520) 已通过：Windows/Linux 核心与同步测试、编辑器类型/构建/浏览器检查，以及 Windows 全量测试、Clippy 和原生窗口回归全部成功。同提交的[安全与许可证检查](https://github.com/xwwpy/StudyApp/actions/runs/39390827422)通过。

[v0.4.0 发布流水线](https://github.com/xwwpy/StudyApp/actions/runs/39391858333)全部成功，并于 2026-10-20 发布 [StudyApp 0.4.0](https://github.com/xwwpy/StudyApp/releases/tag/v0.4.0)。发布门禁重新执行两平台业务测试、浏览器检查、全量 Rust 测试、Clippy、安全审计、许可证检查和原生窗口回归，随后优化构建并打包。`windows-release-smoke` 归档保留截图和运行报告。

从 GitHub 重新下载最终 ZIP 后，文件大小为 14,447,732 字节，SHA256 `8f0f96a5446e2b8959d3e021752bae3822908edad5d99b4f9e959924c3cb0313`，同时匹配下载的 `.sha256` 和 GitHub 资产摘要。下载包解压到全新的 `.scratch/published-v040-046696f66c7443b891c16732bc5a2779` 并启动，其中的正式 EXE 通过五页渲染及离线 WebView2 ready 检查；完整交互断言由上述 CI 的 ui-test 构建验证。发布后的文档提交仅补充验收证据与迁移说明，未修改标签或应用代码。

发布包本地验收：最终优化 EXE 已打包为 `StudyApp-0.4.0-windows-x86_64.zip`（14,447,461 字节、959 个条目），SHA256 `93b1c885c037e1a25c432e7ac2b4365e5592d6a6cb4b1ee9aa0d4cbd9b71e6d1`。ZIP 不含资料库、草稿、同步凭据或 PDB；在新的 `.scratch/package-v040-ac2103e5136e4e39bd97c95d4a7379ad` 解压后实际启动，五页渲染和离线 WebView2 ready 验证通过。GitHub Release 由 CI 独立构建，发布资产以其校验文件为准。

限制：编辑面需要 WebView2 Runtime；系统实际输入法硬件操作没有全矩阵验收，IME 由浏览器 CDP 和正常 native flush 路径验证。原始 HTML/图片/公式/Mermaid 不执行或完整渲染；不声称复刻 Typora 全部功能。单文件 10 MiB，极大单行/复杂嵌套仍可能较慢。真实 WebDAV/NAS、微信、代码签名与自动更新的既有边界不变。历史章节记录对应旧版，不应用于评价 0.4.0 编辑行为。

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
