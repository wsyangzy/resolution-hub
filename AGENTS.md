请始终使用简体中文回复。代码、命令和变量名保持英文。

# Resolution Hub 项目指南

Resolution Hub 是 Windows 专用的游戏分辨率启动器。它在启动游戏前或检测到游戏主进程后切换目标显示模式，并在游戏退出后恢复桌面显示模式。

技术栈：Rust 2024、Tauri 2、windows-rs 0.62、React 19、Vite 8、TypeScript、Tailwind CSS 4。产品目标平台是 Windows 10/11 x64；未明确要求时，不要为其他平台增加抽象或兼容层。

## 代码边界

- `frontend/`：Vite 前端工程；`index.html` 是入口，React/TypeScript 源码位于 `src/`，构建产物输出到仓库根目录 `dist/` 并由 Tauri 加载。
- `frontend/src/lib/tauri.ts`：前端调用 Tauri command、读取窗口/应用 API 和订阅 `runtime-status` 事件的唯一边界。
- `src-tauri/src/lib.rs`：Tauri command 定义、注册和应用入口。
- `src-tauri/src/runtime.rs`：单游戏会话状态机，负责切换、等待、监控、取消和恢复。
- `src-tauri/src/display/winapi.rs`：Windows Display API 适配，包括显示器/模式枚举、切换前测试和实际切换。
- `src-tauri/src/model.rs` 与 `profile.rs`：前后端/持久化数据模型、旧配置兼容、输入校验与 JSON 文件读写。
- `launcher.rs`、`detector.rs`、`process.rs`：启动 EXE/Steam/快捷方式，识别新进程，监控真正的游戏主进程。
- `steam.rs`、`recent.rs`、`autostart.rs`、`dialog.rs`：分别负责 Steam 本地库、最近活动应用、HKCU 开机自启和 Windows 文件选择器。

## 显示模式安全不变量

- 临时切换游戏显示模式前，必须先把恢复目标写入 `pending-restore.json`。恢复成功后才能清除该文件；恢复失败时必须保留它和手动恢复入口。
- 优先恢复用户为该显示器配置的默认模式；它已不受支持时，退回本次切换前的实际模式。
- 游戏会话使用 `display::switch_mode` 做临时切换；用户修改桌面默认模式时才使用 `display::switch_mode_persistent`。不要混用两者。
- 任何切换都要先确认模式仍受当前显示器支持，保留 `CDS_TEST`，并在切换后重读当前模式进行验证。设置文件只能在持久切换验证成功后写入。
- `RuntimeManager` 一次只允许一个会话。`generation` 用来使取消或手动恢复前的 worker 失效；新增等待循环或副作用时也要检查它。
- 发生错误且恢复点已存在时，会话必须尝试恢复。不能因为启动失败、超时、取消或窗口关闭而把桌面留在游戏模式。
- 启动器流程必须监控用户配置的游戏主进程，不得把 Steam、Riot、Epic 等启动器进程的退出当成游戏退出。

`RuntimeStatus.phase` 现有取值为 `idle`、`preparing`、`switching`、`launching`、`waiting`、`running`、`restoring` 和 `error`。新增或改名状态时，同步检查 `frontend/src/` 中的呈现、取消和关闭逻辑。

## 数据与契约

用户数据保存在 `%APPDATA%\resolution-hub\`：

| 文件                     | 用途                   |
| ------------------------ | ---------------------- |
| `profiles.json`        | 游戏配置               |
| `profile-usage.json`   | 托盘快捷启动使用记录   |
| `settings.json`        | 显示器默认模式与全局偏好 |
| `recent-apps.json`     | 最近活动应用历史       |
| `pending-restore.json` | 显示模式异常恢复点     |

- Rust 结构体对前端和 JSON 使用 `#[serde(rename_all = "camelCase")]`。修改字段、枚举或 Tauri command 参数时，同步检查 `model.rs`、`frontend/src/lib/tauri.ts` 和相关 React 调用方。
- 已发布的 JSON 文件就是持久化契约。新增字段要使用 `#[serde(default)]`、明确的默认函数或窄范围迁移保持旧文件可读，并为兼容路径保留一个小型测试。
- Tauri command 名保持 `snake_case`，TypeScript API 保持 `camelCase`。React 组件不得直接调用 `@tauri-apps/api` 或访问 Tauri 全局对象，统一通过 `frontend/src/lib/tauri.ts`。
- React 默认文本渲染会转义动态值；配置、路径、Steam 或进程数据不得使用 `dangerouslySetInnerHTML`，确有需要时必须先在信任边界完成可靠净化。
- 前端表单中的等待时间以分钟输入、Rust 中以秒保存；恢复延迟以秒输入、Rust 中以毫秒保存。修改范围或单位时必须同步前后端校验。

## 实现约定

- 优先直接使用 windows-rs 和 Rust 标准库。`unsafe` 代码保持在小型 Windows 适配函数内，所有成功打开的 Win32 handle 都要在每条返回路径关闭。
- 进程枚举、显示器 API 和注册表访问都可能因权限、进程瞬时退出或设备变化失败；保持边界处错误处理，面向用户的错误信息使用简体中文。
- 不要在 Tauri command 主线程上运行长时间轮询。复用当前 `spawn_blocking` 或受 `generation` 管理的 worker 模式，等待必须有超时或取消出口。
- 前端保持 React、Vite、TypeScript 和 Tailwind CSS 现有架构；优先使用已安装依赖和浏览器原生能力，不为局部效果引入新的框架或大体积依赖。
- 自定义窗口关闭流程会在显示模式仍被接管或表单存在未保存修改时先确认，并在需要时恢复桌面模式。改动标题栏或关闭逻辑时不得绕过受保护的关闭流程。

## 开发与验证

首次安装前端工具后启动开发模式：

```powershell
npm install
npm run dev
```

验证按改动侧选取，不要全跑。前端改动不触发任何 `cargo` 命令，Rust 改动不触发任何 `npm` 命令。

前端侧：

```powershell
npm run typecheck      # 默认
npm test -- ProfileEditor   # 按文件或测试名过滤
npm run build:web      # 触及构建产物或样式时
```

Rust 侧按成本升级，停在能证伪本次改动的最低一档：

```powershell
cargo fmt --check                  # 秒级，先跑
cargo check                        # 不做 codegen/link，签名与类型改动的默认档
cargo test --lib model::           # 改动逻辑有测试覆盖时，按模块过滤
cargo test                         # 跨模块改动，或改了测试代码
cargo build                        # 仅在触及 Windows API 边界或链接配置时
```

升档需要给出理由。纯重构没有行为变化时，`cargo check` 通过即为充分证明。`cargo` 编译在持续输出时不要中断——Tauri + windows-rs 冷编译 5-15 分钟属正常，产物是增量累积的，杀掉重跑只会更慢。

根据改动范围扩大验证：

- 前端样式、布局和视觉效果统一由用户自行验证。Codex 不得使用 Playwright（包括 CLI 和 MCP）、浏览器自动化截图或主动启动前端预览/开发服务进行视觉验收；仅执行 `npm run typecheck`、`npm test`、`npm run build:web` 等非视觉检查，并在交付中明确说明未进行视觉验收。
- 只改文档：先运行 `git diff --check`。样式改动还需运行 `npm run build:web`；760x560 最小尺寸和 980x720 默认尺寸由用户在 Tauri 窗口中自行检查，Codex 不得代为执行。
- 改动 Rust 模型、配置、启动或进程逻辑：运行 `cargo fmt --check` 和 `cargo check`；改动逻辑有测试覆盖时补 `cargo test --lib <module>::`；影响 Windows API 边界时才运行 `cargo build`。
- 改动显示切换、恢复、取消或窗口关闭：自动测试不能证明真实硬件行为，还要在 Windows 上手动验证正常退出、启动超时、取消等待、手动恢复和关闭窗口路径。
- 改动 Tauri command：除 Rust 侧检查外，运行 `npm run typecheck` 和 `npm run build:web`，并核对 `generate_handler!` 与 `frontend/src/lib/tauri.ts`。
- `npm run build`（tauri build，release profile）会重编整棵依赖树且与 dev profile 不共享产物，只能用于发布，绝不作为验证手段；只需要 Release 可执行文件时使用 `npm run build -- --no-bundle`。

版本发布时保持 `src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`、`Cargo.lock` 和关于弹窗中的可见版本一致。
