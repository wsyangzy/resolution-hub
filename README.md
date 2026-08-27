# Resolution Hub

> **Juice 借助 Codex 进行开发。**

[![Release](https://img.shields.io/github/v/release/bmmxat/resolution-hub?display_name=tag&sort=semver)](https://github.com/bmmxat/resolution-hub/releases/latest)
![Platform](https://img.shields.io/badge/platform-Windows%20x64-0078D4)
![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB)
[![License](https://img.shields.io/github/license/bmmxat/resolution-hub)](LICENSE)

Resolution Hub 是一款 Windows 游戏分辨率启动器。它从统一界面启动游戏，
在合适的时机切换目标显示器的分辨率与刷新率，并在游戏退出后自动恢复桌面显示模式。

<p align="center">
  <img src="docs/images/resolution-hub-main.png" alt="Resolution Hub 游戏列表主界面">
</p>

## 下载

前往 [Releases](https://github.com/bmmxat/resolution-hub/releases/latest)
下载最新的 Windows x64 便携版，双击即可运行，无需安装。

运行要求：

- Windows 10/11 x64
- Microsoft Edge WebView2 Runtime（现代 Windows 系统通常已预装）

便携版表示程序无需安装；游戏配置与软件设置仍保存在
`%APPDATA%\resolution-hub`。当前版本未进行商业代码签名，Windows SmartScreen
可能显示安全提示。

## 核心功能

### 按游戏切换显示模式

- 为每个游戏分别设置目标显示器、分辨率和刷新率。
- 支持“启动命令之前”和“检测到游戏主进程时”两种切换时机。
- 游戏退出后自动恢复用户设置的桌面默认模式。
- 支持多显示器，并可为每台显示器保存独立的默认模式。

### 多种游戏添加方式

- **Steam**：扫描本机 Steam 库和已安装游戏，无需 Steam API Key。
- **登录器 / 启动器**：适用于 Riot Client、Epic、Battle.net 等平台。
- **本地游戏**：直接选择游戏可执行文件。
- **最近运行**：从最近活动的应用中识别可执行文件和进程名。

<p align="center">
  <img src="docs/images/add-game.png" alt="添加游戏与选择启动方式">
</p>

启动器模式会等待真正的游戏主进程出现后再切换显示模式，避免在登录、更新阶段
提前修改分辨率。“启动并识别”可帮助首次使用的用户确定游戏主进程，识别期间不会
切换显示模式。

### 守候会话：游戏可以反复启停

从 Resolution Hub 点击启动后会进入运行专注页，**这个页面存在期间会话一直有效**：

- 游戏退出时自动恢复桌面显示模式，页面不关闭，继续等待该游戏再次启动。
- 再次启动游戏（可以直接从启动器点“开始游戏”，不必回到 Resolution Hub）会自动重新切换显示模式。
- 点击“结束会话”才停止监控并恢复桌面；已经在运行的游戏不会被关闭。

启动器类的游戏尤其适用：Resolution Hub 打开启动器后就在专注页守着，你在启动器里进出游戏多少次都会被正确接管。

会话期间主窗口可以隐藏到系统托盘，托盘菜单会显示当前状态。

### 分辨率与刷新率二级选择

显示器选择固定在面板顶部，分辨率按像素大小排序。展开一个分辨率后，再选择该
分辨率支持的刷新率，更贴合以修改分辨率为主的使用场景。

<p align="center">
  <img src="docs/images/resolution-picker.png" alt="默认分辨率与刷新率选择面板">
</p>

### 恢复与安全

- 切换前验证目标显示模式是否仍受当前显示器支持。
- 按游戏主进程监控运行状态，避免将启动器退出误判为游戏退出。
- 游戏窗口按切换前的分辨率创建时（画面只显示左上角一块），自动把窗口贴合回目标显示器。
- 保存异常恢复点；程序意外退出后，下次启动会提示恢复桌面模式。
- 支持配置等待超时，随时可结束会话并“立即恢复”。
- 支持当前用户的 Windows 开机自启开关。
- 系统托盘支持快速启动最常用的 5 个游戏、手动恢复和安全退出。

## 使用流程

1. 添加游戏并选择 Steam、启动器、本地游戏或最近运行。
2. 选择游戏主进程、目标显示器、分辨率和刷新率。
3. 从 Resolution Hub 启动游戏，进入运行专注页。
4. 程序在配置的时机或检测到游戏进程后切换显示模式。
5. 游戏退出后恢复桌面默认模式，并继续守候下一次启动，直到你结束会话。

## 技术栈

- Rust 2024
- Tauri 2
- windows-rs
- React 19
- Vite 8
- TypeScript
- Tailwind CSS 4

## 开发

安装前端工具并启动 Tauri 开发模式：

```powershell
npm install
npm run dev
```

仅启动浏览器中的前端开发服务器：

```powershell
npm run dev:web
```

仅编译开发版：

```powershell
cargo build
```

开发可执行文件位于 `target\debug\resolution-hub.exe`。

### 测试

```powershell
npm run typecheck
npm test
npm run build:web
cargo fmt --check
cargo test
```

前端源码位于 `frontend\src`，Vite 构建产物输出到 `dist`。所有 Tauri API
调用和 `runtime-status` 事件订阅统一收口在 `frontend\src\lib\tauri.ts`。

### 构建

生成 NSIS 安装包：

```powershell
npm run build
```

仅生成免安装的 Release 可执行文件：

```powershell
npm run build -- --no-bundle
```

Release 可执行文件位于 `target\release\resolution-hub.exe`。

## 本地数据

用户数据保存在 `%APPDATA%\resolution-hub\`：

| 文件 | 用途 |
| --- | --- |
| `profiles.json` | 游戏配置 |
| `profile-usage.json` | 托盘快捷启动排序所用的本地计数 |
| `settings.json` | 默认桌面模式与全局应用偏好 |
| `recent-apps.json` | 本机最近活动应用历史 |
| `pending-restore.json` | 异常退出时的临时恢复点 |

Steam 游戏列表仅读取本机 Steam 注册表、`libraryfolders.vdf` 和
`appmanifest_*.acf`。开机自启使用当前用户注册表项：

```text
HKCU\Software\Microsoft\Windows\CurrentVersion\Run
```

## 当前范围

- 会话必须由 Resolution Hub 发起；结束会话后不再监控游戏进程。
- 启动器模式依赖游戏主进程名；首次添加时建议使用“启动并识别”。
- 支持关闭主窗口后驻留系统托盘；当前不包含开机后的后台静默启动。
- 仅面向 Windows，显示模式切换依赖 Windows Display API。

## 许可证

本项目基于 [MIT License](LICENSE) 开源。
