# 验证记录

验证日期：2026-10-19。Windows x64，Rust 1.98.0，GPUI Kit 0.7.3，使用提交的 Cargo.lock。

## 本地执行

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

## 远端执行

首次推送 `e0d203f` 的 [Security and licenses](https://github.com/xwwpy/StudyApp/actions/runs/39261248613) 已通过；[CI](https://github.com/xwwpy/StudyApp/actions/runs/39261248603) 的 Windows/Linux 核心测试已通过，Windows 桌面检查仍在执行。本地随后修复了独立代码审查发现的离线恢复归属和版本关联问题，上述 45 项测试与增强后的原生冒烟已覆盖这些修复。最终提交、标签对应的远端结果以 [Actions](https://github.com/xwwpy/StudyApp/actions) 和 [Release](https://github.com/xwwpy/StudyApp/releases/tag/v0.1.0) 页面为准；工作流源码本身不作为执行成功证据。

## 范围

当前交付为 Windows 本地版本。微信登录、云端保存、双端同步保留接口与设置入口，尚未接入后端；没有自动更新和代码签名。系统通知还受 Windows 通知权限影响。文件历史为运行时轮询快照，不能还原关闭应用期间的每次编辑。
