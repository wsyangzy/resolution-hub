请始终使用简体中文回复。代码、命令和变量名保持英文。

# Resolution Hub 项目指南

Resolution Hub 是 Windows 专用的游戏分辨率启动器。技术栈为 Rust 2024、Slint 1.17、
windows-rs 0.62；产品目标是 Windows 10/11 x64。未明确要求时，不增加其他平台适配层。
React/Tauri 已退出当前源码，迁移前版本保留在 Git 历史中。

## 代码边界

- `core/src/runtime.rs`：单游戏会话状态机，负责切换、等待、监控、取消和恢复。
- `core/src/display/winapi.rs`：Windows Display API，模式枚举、测试、切换和结果验证。
- `core/src/model.rs`、`profile.rs`：持久化模型、旧配置兼容、校验与 JSON 读写。
- `core/src/launcher.rs`、`detector.rs`、`process.rs`：启动来源、识别与监控真正的游戏主进程。
- `core/src/steam.rs`、`recent.rs`、`dialog.rs`、`icon.rs`：本机来源、文件选择和图标提取。
- `native-ui/src/main.rs`：Slint Controller 与应用入口，复用唯一的 RuntimeManager。
- `native-ui/src/state.rs`、`editor.rs`、`presentation.rs`：界面状态、草稿校验与呈现模型。
- `native-ui/src/chrome.rs`、`placement.rs`：Windows 窗口关系、工作区、DPI 和几何管理。
- `native-ui/src/runtime_reader.rs`、`icons.rs`、`preferences.rs`：后台读取、图标缓存与有序保存。
- `native-ui/ui/`：Slint 组件、样式与动效；具体约束见 `native-ui/DESIGN.md`。
- `native-ui/scripts/`：供用户手动运行的性能采样与汇总，不自动启动。
- `assets/resolution-hub.ico`、`native-ui/build.rs`：EXE 图标、版本资源与 Common-Controls 清单。

`core` 不得依赖界面框架、WebView 或 native-ui。状态通过 `StatusSink` 回调传出，
界面更新必须回到 Slint 事件循环；两个窗口共享一个状态和一个运行会话。

## 显示模式安全不变量

- 临时切换前必须先写入 `pending-restore.json`。仅恢复成功后才能清除；失败保留恢复点和重试入口。
- 优先恢复用户为显示器保存的默认模式；不再受支持时退回本次切换前的实际模式。
- 游戏会话使用 `display::switch_mode` 临时切换；修改桌面默认模式才使用 `switch_mode_persistent`。
- 保留支持性检查、`CDS_TEST` 和切换后重读验证。只有持久切换验证成功后才可保存设置。
- RuntimeManager 一次只允许一个会话。新增循环或副作用必须遵守 `generation` 失效与取消规则。
- 启动失败、超时、取消或关闭时，只要存在恢复点，就必须尝试恢复。
- 监控用户配置的游戏主进程，不能把 Steam、Riot、Epic 等启动器退出当成游戏退出。

RuntimeStatus.phase 为 `idle`、`preparing`、`switching`、`launching`、`waiting`、`running`、
`restoring` 和 `error`。调整状态时同步检查原生界面的呈现、取消和关闭逻辑。

## 数据与兼容

数据位置为 `%APPDATA%\resolution-hub\`：`profiles.json`、`profile-usage.json`、`settings.json`、
`recent-apps.json`、`pending-restore.json` 和 `native-ui-settings.json`。

- JSON 字段保持 `#[serde(rename_all = "camelCase")]`，已发布的文件格式就是兼容契约。
- 新字段使用 serde 默认值或窄范围迁移，并对兼容路径保留有意义的小型测试。
- 不因 Slint 暂无设置入口而删除旧版偏好、默认模式或启动记录字段。
- 等待时间界面以分钟输入、核心以秒保存；恢复延迟界面以秒输入、核心以毫秒保存。
- 来源目录、游戏路径和进程数据只能作为数据呈现，不拼入可执行代码或命令文本。

## 实现约定

- 优先使用 windows-rs、标准库和已安装依赖；unsafe 限制在小型 Windows 适配函数中，所有成功打开的句柄必须关闭。
- 设备变化、进程瞬时退出、注册表和权限错误必须在边界处理，用户错误信息使用简体中文。
- 文件、进程、显示 API 与锁等待不阻塞 Slint 事件循环。后台结果需校验代次，迟到回调不得覆盖新状态。
- 保留虚拟列表、模型缓存与图标按行更新，不因局部输入重建隐藏面板或整个游戏模型。
- 受保护的关闭流程必须处理未保存草稿、正在识别和活动会话，恢复失败保留窗口与恢复点。
- 当前尚无编辑/删除配置、托盘、自启或桌面设置入口；不要把旧版能力误报为 Slint 已实现。

## 验证与交付

按当前约定，编译、测试和发布由推送后的 GitHub Actions 执行。
未经用户另行要求，不在本地执行 `cargo check`、`cargo test`、`cargo build` 或 `cargo run`。
本地可以执行不编译的 `cargo fmt --all --check`、`cargo metadata --no-deps`、锁文件检查、脚本语法解析和 `git diff --check`。
不要恢复 npm、Vite 或 Tauri 构建流程。

远程 Windows CI 必须使用锁文件，检查 Rust 格式、运行工作区测试，并为发布构建 Slint Release EXE。
`ci.yml`、`nightly.yml` 和 `release.yml` 分别负责验证、定时/手动预发布和版本标签正式发布。
共用构建步骤位于 `.github/actions/build-windows/action.yml`，复用 `scripts/Prepare-Executable.ps1` 检查单文件产物。
CI 不发布；Nightly 仅在 main 定时或手动运行；Release 标签必须与工作区版本一致，不覆盖已发布的正式版本。
发布验证应检查 x64 PE、GUI 子系统、图标、版本资源和嵌入的 Common-Controls v6 清单。
Windows x64 统一使用 `.cargo/config.toml` 静态链接 CRT；保留常规和延迟导入的 VC++ 依赖检查。
Nightly 仅在同一提交已经有完整单 EXE 预发布时跳过，手动 `force` 可以重建；缺失或不完整的发布必须重试。
测试和构建通过不等于真实显示切换或视觉验收。

界面、布局、动画和 DPI 由用户在 Windows 原生窗口中验收。
Codex 不得使用 Playwright、浏览器截图或主动启动程序、预览/开发服务代为视觉验收。
显示切换或关闭流程改动还需由用户覆盖正常退出、启动超时、取消、手动恢复和关闭窗口路径。
不得自动启动游戏或操作实际显示模式。

版本统一修改根 `Cargo.toml` 的 `workspace.package.version`，同步锁文件与显示版本。
保留用户数据、本地缓存和不属于本次任务的文件，不使用广泛清理或回退。
