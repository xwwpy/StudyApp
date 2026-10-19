# 拾光学习 · StudyApp

使用 GPUI Kit 0.7.3 构建的 Windows 中文学习桌面应用。导入自己的 Markdown 笔记，定时确认学习内容，安排间隔复习，并查看本地学习记录与版本历史。无需账户即可使用。

当前为 **0.1.0 本地首版**：微信登录、云端保存和双端同步尚未连接服务，设置页明确显示未配置；这些能力不能作为已实现功能使用。后续边界见 [cloud-contract.md](docs/cloud-contract.md)。

![StudyApp 今日学习原生界面](docs/images/dashboard.png)

## 启动

从 [GitHub Releases](https://github.com/xwwpy/StudyApp/releases) 下载可用的 Windows x64 ZIP，解压后运行 `studyapp.exe`。ZIP 是免安装程序包，默认资料保存在用户应用数据目录，而非解压目录。发布程序**未进行代码签名**；尚无自动更新器。首次发布需维护者推送与版本一致的 `v0.1.0` 标签，工作流成功后才会出现下载。

支持 Windows 10/11 x64，使用 Windows 图形渲染；远程桌面、显卡驱动及受限运行环境可能影响窗口创建。若系统提示缺少 MSVC 运行库，安装微软 [Visual C++ x64 Redistributable](https://aka.ms/vs/17/release/vc_redist.x64.exe)。Linux CI 只测试核心业务，不提供 Linux 桌面程序。

开发构建需要 Rust **1.98.0**、Visual Studio 2022 C++ 桌面开发工具（含 Windows SDK）、CMake 和 PowerShell 7。在仓库根目录运行：

```powershell
cargo build --locked -p studyapp-desktop
cargo run --locked -p studyapp-desktop
```

单独指定资料库目录：

```powershell
.\target\debug\studyapp.exe --data-dir "D:\StudyAppData"
```

## 使用与规则

1. 在「笔记库」导入 `.md` / `.markdown`，可以多选、搜索、查看内容及版本差异。支持 UTF-8 / UTF-8 BOM，单文件上限 10 MiB；导入结果逐项报告失败。
2. 应用运行时累计学习时间，可以暂停。默认每 **60 分钟**询问一次，可设为 **5–240 分钟**；修改间隔从下一段生效。
3. 时间段结束会显示待回答卡片并尝试发送 Windows 通知。默认 **5 分钟**内填写摘要、确认关联笔记，可设为 **1–30 分钟**。拒绝回答或到期未答会记为「未学习」；重启时也会结算已过期记录。
4. 提醒等待时下一段继续计时；关闭、暂停和休眠期间不新增学习时长。正常关闭保存当前累计时间，运行中每 5 秒检查点，异常退出可能损失最后数秒。
5. 确认学习后创建独立复习计划，默认间隔依次为 **1、3、7、14、30 天**。下一阶段从实际复习完成时间起算；逾期保留，勾选可完成，支持撤销最近一次复习。设置支持 1–20 阶段、每阶段 1–3650 天，只影响新计划。
6. 统计按当前本地自然日分组，跨日时按实际时间分配。可以手动补记和更正学习记录，学习时长来自用户确认，不能视为对实际专注程度的测量。

文件只读：应用不会改写、删除原始 Markdown，也不会在笔记目录初始化 Git。后台每 **3 秒轮询**已导入文件，发现变化后要求内容稳定至少 **3 秒**再保存快照，因此变化通常需要约 3–6 秒被记录。以哈希去重；首次导入只建立基线，不自动算学习。应用关闭期间无法捕获每次编辑，下次启动只记录最后快照与当前文件之间的离线变化。文件失联保留历史，并支持重新关联。

预览支持常见 Markdown、代码和表格，原始 HTML 显示为文本；图片仅显示说明，链接仅显示文字，不执行脚本、不自动加载远程图片、不打开链接。版本历史会增加数据库大小，首版没有历史自动清理。

## 数据位置、迁移和恢复

默认数据库：`%APPDATA%\StudyApp\StudyApp\data\studyapp.sqlite3`。在「设置」查看实际路径；SQLite 同目录可能出现 WAL/SHM 文件及 `studyapp.lock`（运行中的互斥锁）。同一资料库不能被两个窗口同时打开。

「选择资料库目录」切换到该目录的独立资料库，原资料仍留在原处。普通启动将选择结果以 JSON 字符串保存到 `%APPDATA%\StudyApp\StudyApp\config\location.json`，后续启动会使用该目录。`--data-dir` 优先于此配置，并且该次运行的目录切换不会修改全局启动位置。需要回到默认目录时，可在设置中选择默认数据目录。

迁移已有资料时，先在旧资料库「导出备份」，再切换到目标目录并「从备份恢复」。备份使用 SQLite 在线备份 API，导出为单个一致的数据库文件，要求使用新文件名。恢复会验证应用标识、格式、数据库完整性和业务约束，先在当前资料库目录生成 `studyapp-before-restore-时间-唯一ID.sqlite`，再事务性恢复；不会删除资料目录。数据库升级由版本化 schema 管理，无法识别的较新格式会拒绝读取，升级前建议导出备份。

备份包含笔记内容快照、学习记录、设置和复习计划，**不复制原始 Markdown 文件**。换电脑后仍可查看快照，但应另外复制原始笔记并重新关联路径。数据库和备份包含全文及本机路径，没有应用级加密，请自行保管。运行中请使用应用导出备份，避免只复制主数据库而遗漏 WAL 内容。

## 测试和打包

```powershell
cargo fmt --all -- --check
cargo test --locked -p studyapp-core
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
cargo build --locked -p studyapp-desktop --features ui-test
$smoke = Join-Path $env:TEMP ('studyapp-smoke-' + [guid]::NewGuid().ToString('N'))
.\target\debug\studyapp.exe --smoke-test --data-dir $smoke
```

冒烟测试必须使用新目录；已有数据库会被拒绝，防止覆盖资料。该模式创建隔离笔记和记录，渲染五个原生页面、点击导航、复习勾选、撤销、暂停和保存设置，并通过后台命令验证备份、恢复后的设置及界面刷新。启用 `ui-test` 才会输出 `page-0.png` 至 `page-4.png` 和执行控件断言，成功后输出 `smoke-result.txt` 并退出。`scripts/smoke.ps1 -DataDirectory 新路径` 与 CI 使用同一验证入口，会检查退出状态、报告和截图，保留测试目录作为证据。

```powershell
cargo build --locked --release -p studyapp-desktop
.\scripts\package.ps1 -Version 0.1.0
```

脚本生成 `dist/StudyApp-0.1.0-windows-x86_64.zip` 与 `.zip.sha256`，包含程序、README、MIT LICENSE、发布说明、锁文件及第三方许可证/源代码下载链接。输出已存在时拒绝覆盖，可用 `-OutputDirectory` 指定新目录；不会删除文件。`-BinaryPath` 允许验证打包其他已构建程序。SHA256 可用 `Get-FileHash -Algorithm SHA256` 核对。

## CI/CD 与依赖

[CI](https://github.com/xwwpy/StudyApp/blob/main/.github/workflows/ci.yml) 在 PR、`main` 和 `codex/**` 推送时检查格式、Windows/Linux 核心测试、Windows 全工作区 Clippy 和测试、原生构建及隔离 UI 冒烟。截图和结果即使失败也尝试上传；Linux 无 GPU 要求。

[Security](https://github.com/xwwpy/StudyApp/blob/main/.github/workflows/security.yml) 在上述事件及每周运行 `cargo-audit 0.22.2` 和 `cargo-deny 0.20.2`。审计读取提交的 Cargo.lock 和当前 RustSec 数据，不忽略已知漏洞；许可证/来源检查限定支持的 Windows 依赖图，重复版本警告保留。Dependabot 每周检查 Cargo 和固定提交的 Actions。框架传递依赖的告警需要持续跟进，不能把构建通过等同于安全审计通过。

2026-10-19 的本地 RustSec 审计报告 **0 个已知漏洞**，同时保留 `instant`、`paste`、`rustls-pemfile`、`rustybuzz`、`ttf-parser`、`zbus_names`、`zvariant`、`zvariant_derive` 的停止维护警告。这些来自框架传递依赖，应继续跟踪上游升级；没有添加忽略规则。该结果是当日数据库快照，后续 CI 会使用更新的公告。

[Release](https://github.com/xwwpy/StudyApp/blob/main/.github/workflows/release.yml) 仅由 `v*` 标签触发，验证标签等于所有工作区包版本，通过格式、测试、Clippy、依赖审计和原生 UI 冒烟后构建 Windows release ZIP 和 SHA256，使用最小权限发布 GitHub Release。Actions 固定完整提交，只有发布 job 拥有仓库写权限。工作流定义不代表已在 GitHub 执行成功；实际验证见 [verification.md](docs/verification.md)。

源码采用 [MIT](LICENSE)；依赖遵循各自许可。未来微信/云端还需要独立后端、域名、凭据、安全会话存储及端到端验证，当前没有可部署的云服务。
