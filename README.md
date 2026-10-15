# Resolution Hub

Resolution Hub 是 Windows 10/11 x64 游戏分辨率启动器，使用 Rust、Slint 和 Windows Display API。
它在启动游戏前或检测到游戏主进程后切换目标显示器的分辨率与刷新率，并在游戏退出后恢复桌面模式。

本项目由 Juice 借助 Codex 开发。当前已迁移为 Slint 原生界面，运行不再依赖 WebView2、Node.js 或浏览器前端。

## 下载与构建

从 [Nightly release](https://github.com/wsyangzy/resolution-hub/releases/tag/nightly)
下载单个 `resolution-hub.exe` 后直接运行，无需解压或安装。
图标和界面资源已内嵌，程序旁不需要 `build-info.json`、说明文件或其他附带文件。
版本、来源提交、构建记录和 EXE SHA256 保存在发布页；发布资产仅提供这个 EXE。
窗口标题显示版本，Nightly 构建额外标注 nightly 渠道；EXE 属性中保留统一版本号。
新版沿用显示器与双箭头图形，主色改为蓝紫色，PNG 窗口图标与多尺寸 ICO 同步使用。
便携版仍将用户数据保存在 `%APPDATA%\resolution-hub\`。

三个工作流分别触发，构建步骤复用本地 composite action：

| 工作流 | 触发条件 | 结果 |
| --- | --- | --- |
| [CI](https://github.com/wsyangzy/resolution-hub/actions/workflows/ci.yml) | `main` 推送、目标为 `main` 的 PR、手动 | 格式、测试、构建和 EXE 检查，保留验证产物 |
| [Nightly](https://github.com/wsyangzy/resolution-hub/actions/workflows/nightly.yml) | 每日 02:23 UTC、手动选择 `main` | 有未发布变更时验证并更新 `nightly` 滚动预发布 |
| [Release](https://github.com/wsyangzy/resolution-hub/actions/workflows/release.yml) | 推送 `v<版本>` 标签 | 标签与工作区版本一致时，验证并发布正式版 |

Release 也可手动执行构建验证，手动运行不会发布正式版。正式发布前先更新根 Cargo 版本和锁文件，
将改动合入 `main` 后再推送对应标签。已发布的正式版不覆盖，后续更新使用新版本标签。
CI 不发布版本，普通分支推送不触发 Nightly 或 Release，既有稳定版标签保持不变。
Nightly 先检查来源提交与发布资产，同一提交已有完整单 EXE 时跳过；手动运行可勾选 `force` 重建。
发布缺失、仍为草稿或资产不完整时会重新构建，不能仅凭 `nightly` 标签存在就跳过。

CI 先在独立快速任务中检查 Rust 格式和打包脚本，再在 Windows runner 上使用锁文件
执行工作区 Release 测试和 Slint EXE 构建。测试步骤限时 40 分钟，EXE 构建限时 15 分钟，
失败时保留对应日志；新 PR 推送会取消旧验证，`main` 的每次验证记录保留。
发布前检查 x64 PE、GUI 子系统、图标、版本和 Common-Controls v6 清单，
同时检查常规与延迟加载的 DLL 导入，拒绝依赖外部 VC++ 运行库的 EXE。
Windows x64 构建统一静态链接 CRT；用户不需要额外安装 Visual C++ Redistributable。
外部 Actions 固定到支持 Node.js 24 的提交；Dependabot 每周检查 Cargo 与 Actions 依赖，更新通过 PR 进入相同 CI。
下载包在 Actions artifacts 中保留 30 天，可在新版本出现问题时取回上一次成功产物；
替换程序前先结束会话并正常退出，保留用户数据，恢复失败时不要删除恢复点。

## 使用方式

1. 在工作条或应用窗口中点击添加，选择 Steam、启动器、本地 EXE 或最近运行。
2. 设置游戏主进程、目标显示器、分辨率与刷新率；需要时使用“启动并识别”确定真正的游戏进程。
3. 保存后选中游戏并启动，程序按配置的时机切换显示模式。
4. 游戏退出后恢复桌面模式，会话继续守候该游戏再次启动。
5. 点击“结束/恢复”停止守候；游戏本身不会被关闭。

启动器流程监控游戏主进程，不把 Steam、Riot、Epic 等启动器退出当成游戏退出。
启动器已经运行时会尝试复用。首次识别只打开启动来源并观察进程，不切换显示模式。

## 原生界面

- **工作条**：688×56 逻辑像素，游戏列表、详情、添加与布局设置按需展开。
- **应用窗口**：可缩放、最大化，常驻游戏列表，与工作条共享同一会话和选择。
- Steam 与最近来源列表使用可见行渲染，图标在后台加载；文件选择、来源枚举和进程识别不占用界面线程。
- 首次打开浮层先完成目标尺寸下的绘制，再开始进入动画；支持 Windows 关闭动画设置。
- 布局选择会保存，窗口位置和尺寸暂不持久化。

详情见 [原生界面说明](native-ui/README.md) 和 [设计约束](native-ui/DESIGN.md)。

## 当前范围

支持读取已有配置、新增游戏、启动守候、结束会话、异常恢复与安全关闭。
当前 Slint 界面尚未提供已有配置编辑或删除、桌面默认显示模式设置、托盘和开机自启入口。
旧版已经保存的桌面默认模式继续用于恢复；未配置时使用本次切换前的实际模式。
旧版自启注册项不会由新版修改，曾启用自启的用户可以在 Windows 启动应用设置中管理。

React/Tauri 源码、旧安装配置和旧界面截图已移除。迁移前源码保留在 Git 历史的
`1b670d5` 提交中；运行旧版前应先在当前程序结束会话并正常退出，避免两个实例同时接管显示模式。

## 显示恢复

切换前会验证显示器仍支持目标模式，并先保存 `pending-restore.json`。
恢复成功后才清除恢复点，恢复失败会保留文件与手动恢复入口。
关闭窗口时有活动会话或未保存草稿会先确认，恢复未完成时不会直接退出。
重新启动发现异常恢复点时，先恢复桌面，再允许启动游戏。

真实的模式切换、DPI、多显示器及动画流畅度需要在 Windows 窗口中验证，单元测试不能替代硬件验收。

## 源码结构

| 路径 | 职责 |
| --- | --- |
| `core/` | 显示模式、配置兼容、游戏启动、主进程识别、单会话状态机 |
| `native-ui/` | Slint 界面、后台任务调度、窗口管理、图标与性能诊断 |
| `assets/resolution-hub.ico` | Windows 可执行文件图标 |
| `Cargo.toml` | 工作区、共享版本与构建配置 |

版本统一由根 `Cargo.toml` 的 `workspace.package.version` 提供，两个包和 EXE 版本资源均沿用该值。
构建、测试与发布交由 GitHub Actions，不在本地重复编译。

## 本地数据

数据目录保持为 `%APPDATA%\resolution-hub\`，迁移不会删除或重置已有配置。

| 文件 | 用途 |
| --- | --- |
| `profiles.json` | 游戏配置 |
| `profile-usage.json` | 保留的启动使用记录 |
| `settings.json` | 桌面默认模式与兼容的旧版偏好 |
| `recent-apps.json` | 最近活动应用历史 |
| `pending-restore.json` | 显示模式异常恢复点 |
| `native-ui-settings.json` | 原生界面布局偏好 |

Steam 来源仅读取本机注册表、`libraryfolders.vdf` 和 `appmanifest_*.acf`，不需要 Steam API Key。

## 许可证

本项目基于 [MIT License](LICENSE) 开源。
