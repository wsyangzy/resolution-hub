# Resolution Hub

Resolution Hub 是一款 Windows 游戏分辨率启动器。它从统一界面启动游戏，在合适的
时机切换目标显示器的分辨率与刷新率，并在游戏退出后自动恢复桌面显示模式。

技术栈：Rust 2024、Tauri 2、windows-rs 和原生 HTML/CSS/JavaScript。

## 工作流程

1. 添加游戏并选择启动来源。
2. 选择游戏主进程、目标显示器、分辨率和刷新率。
3. 从 Resolution Hub 启动游戏。
4. 程序在启动前或检测到真实游戏进程后切换显示模式。
5. 游戏退出后，程序按配置延迟恢复桌面默认模式。

## 当前功能

### 游戏启动与识别

- 支持直接 EXE、Steam App ID 和 Windows 快捷方式。
- 自动扫描本机 Steam 注册表、所有 Steam 库与已安装游戏。
- 支持 Riot Client、Epic、Battle.net 等需要登录或更新的启动器。
- 启动器模式等待真实游戏进程出现后再切换，避免登录器阶段提前修改分辨率。
- “启动并识别”可辅助用户确定游戏主进程，识别期间不会切换显示模式。
- 可从最近活动应用中选择 EXE 和进程名。
- 内置无畏契约预设，默认识别 `VALORANT-Win64-Shipping.exe`。
- 支持取消启动等待，并可配置等待超时。
- 游戏已经运行时，可以直接接管其显示模式。

### 显示模式管理

- 枚举 Windows 当前活动显示器、物理显示器型号和当前显示模式。
- 支持多显示器，并可为每台显示器保存独立的桌面默认模式。
- 分辨率按像素大小排序，点击分辨率后再选择对应刷新率。
- 支持“启动命令之前”和“检测到游戏进程时”两种切换时机。
- 游戏退出后可配置恢复延迟。
- 游戏退出后优先恢复用户设置的默认模式；未设置时恢复启动前模式。
- 支持手动“立即恢复”。

### 恢复与安全

- 切换前验证目标显示模式是否仍受当前显示器支持。
- 按游戏主进程监控运行状态，避免将启动器退出误判为游戏退出。
- 保存异常恢复点；程序意外退出后，下次启动会提示恢复桌面模式。
- 启动等待期间可取消，切换失败时在界面显示错误信息。

### 界面与系统集成

- 游戏配置支持新增、编辑和删除。
- 默认分辨率使用右侧抽屉，显示器位于顶部，分辨率与刷新率采用二级选择。
- 支持暗色和浅色主题、无边框窗口及应用内统一滚动条样式。
- 关于界面显示版本、技术栈和开发者信息。
- 支持当前用户的 Windows 开机自启开关。

## 系统要求

- Windows
- Rust 工具链
- Node.js 与 npm
- Microsoft Edge WebView2 Runtime

## 开发运行

安装前端工具并启动 Tauri 开发模式：

```powershell
npm install
npm run dev
```

仅编译开发版：

```powershell
cargo build
```

开发可执行文件位于：

```text
target\debug\resolution-hub.exe
```

## 测试

```powershell
node --check frontend\js\api.js
node --check frontend\js\app.js
cargo fmt --check
cargo test
```

## 构建

Tauri 配置当前以 NSIS 为正式安装包目标：

```powershell
npm run build
```

便携版可执行程序属于单独的发布流程，不由开发构建自动生成。

## 本地数据

用户数据保存在：

```text
%APPDATA%\resolution-hub\
```

主要文件：

- `profiles.json`：游戏配置。
- `settings.json`：每台显示器的默认桌面模式。
- `recent-apps.json`：本机最近活动应用历史。
- `pending-restore.json`：异常退出时的临时恢复点。

Steam 游戏列表仅读取本机 Steam 注册表、`libraryfolders.vdf` 和
`appmanifest_*.acf`，不需要 Steam Web API 或 API Key。

开机自启使用当前用户注册表项：

```text
HKCU\Software\Microsoft\Windows\CurrentVersion\Run
```

## 当前范围

- 目前以“由 Resolution Hub 启动游戏”为主要使用方式。
- 启动器模式依赖游戏主进程名；首次添加时建议使用“启动并识别”。
- 当前不包含系统托盘和后台静默启动。
- 仅面向 Windows，显示模式切换依赖 Windows Display API。
