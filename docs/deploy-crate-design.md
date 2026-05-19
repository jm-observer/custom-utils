# util_updater 模块设计文档

`custom-utils` 的 `util_updater` 模块，为 Linux 系 CLI 项目提供一套**统一、全程免 root** 的部署能力：GitHub 自更新、用户级 systemd 服务安装、watchdog 心跳衔接、workspace/配置路径解析。公开路径为 `custom_utils::updater`。

核心入口是 [`LinuxService`]：**配置一次，派生四件自洽的事**。四块功能此前各自发明了一套路径（`util_args` 用 `$HOME/.config/{app}`、`ServiceConfig` 用 `/usr/local/bin` + 无 home 系统用户、`UpdateConfig` 写 `current_exe().parent()`），互相矛盾——部署成系统服务后系统用户无 `$HOME`、自更新写 `/usr/local/bin` 需 root。本模块收敛到**单一、面向当前登录用户的布局**：

- 二进制：`~/.local/bin`
- workspace/配置：`~/.config/<app>`（与 `custom_utils::args::workspace` 运行时解析一致）
- 服务：**用户级 `systemctl --user`**，归属当前登录用户
- 单元文件：`~/.config/systemd/user/<name>.service`
- **install / 运行 / 自更新全程不需要 root**

> 实现说明：全程 async `reqwest`，未引入 `self_update`（与「禁止阻塞 API」规则一致）；无 root 检测、无 `chown`/`useradd`、无额外原生依赖、无 `unsafe`。

---

## 依赖与 feature

依赖工作区已有的 `anyhow` / `reqwest` / `serde_json` / `futures-util` / `tokio` / `home`，无新增第三方 crate。

**单一 feature** 给全套：自更新 + 用户级 systemd 安装 + watchdog + `LinuxService`/`handle_cli`。

```toml
[features]
updater = ["tokio", "libsystemd"]       # Linux 部署全栈，一个开关
daemon-async = ["libsystemd", "tokio"]  # 仅要独立 watchdog（async），不依赖 updater
daemon-sync = ["libsystemd"]            # 仅要独立 watchdog（sync）
```

`updater` 已捆绑 async watchdog，故 `LinuxService::spawn_watchdog` 在 `updater` 下恒可用（返回 `tokio::task::JoinHandle<()>`）；真正发送心跳仍需 Linux + `prod`（见 `util_daemon`）。`daemon-async`/`daemon-sync` 仅供「只要 watchdog、不要 updater」的独立场景；`daemon-sync` 与 `updater`/`daemon-async` 互斥（同时启用时优先 async，避免 glob 冲突）。用户级服务同样支持 `sd_notify`，watchdog 对 `systemctl --user` 有效。

> 接入方接入：`custom-utils = { version = "0.13", default-features = false, features = ["updater"] }`

---

## 模块结构

```
src/util_updater/
├── mod.rs            # pub use LinuxService / ServiceConfig / UpdateConfig / UpdateOutcome
├── linux_service.rs  # LinuxService —— 统一聚合入口
├── update.rs         # GitHub Release 自更新（async）
└── systemd.rs        # 用户级 systemd 服务安装
```

---

## API 设计

### 1. 统一入口 `LinuxService`（`linux_service.rs`）

单一数据源，派生 `service_config()` / `update_config()` / `workspace()` / `bin_dir()` / `self_update()` / `install()` / `spawn_watchdog()`，保证四块功能路径一致。

```rust
impl LinuxService {
    pub fn new(
        app: impl Into<String>,        // 单元名 + ~/.config/<app> 段
        repo_owner: impl Into<String>,
        repo_name: impl Into<String>,
        version: impl Into<String>,
    ) -> Self;

    pub fn bin_name(self, name: impl Into<String>) -> Self;   // 默认 = app
    pub fn extra_bins<I, S>(self, bins: I) -> Self;
    pub fn user(self, user: impl Into<String>) -> Self;       // 默认当前登录用户
    pub fn user_home(self, home: impl Into<PathBuf>) -> Self;
    pub fn workspace_arg(self, path: impl Into<String>) -> Self; // 覆盖 ~/.config/<app>，~ / ./ 展开
    pub fn description(self, desc: impl Into<String>) -> Self;
    pub fn exec_args(self, args: impl Into<String>) -> Self;  // 默认 "-w {workspace}"
    pub fn restart_sec(self, secs: u32) -> Self;
    pub fn watchdog_sec(self, secs: u32) -> Self;             // 开启 Type=notify + WatchdogSec

    pub fn service_config(&self) -> ServiceConfig;
    pub fn update_config(&self) -> anyhow::Result<UpdateConfig>;  // install_dir = ~/.local/bin
    pub fn workspace(&self) -> anyhow::Result<PathBuf>;
    pub fn bin_dir(&self) -> anyhow::Result<PathBuf>;
    pub async fn self_update(&self, force: bool) -> anyhow::Result<UpdateOutcome>;
    pub fn install(&self) -> anyhow::Result<()>;

    // ---- 透传式 CLI 集成（库不读 argv、不碰 stdout）----
    /// argv → DeployCommand（util_args 轻量解析）；None = 非 deploy，宿主自理
    pub fn parse_deploy(&self) -> Option<DeployCommand>;
    /// 执行某个 DeployCommand（宿主透传变体转发进来）
    pub async fn dispatch(&self, cmd: DeployCommand) -> anyhow::Result<CliAction>;
    /// deploy 子命令用法文本（宿主与自己的 help 拼接打印）
    pub fn deploy_usage(&self) -> String;
    /// 零配置糖 = parse_deploy + dispatch，未匹配回退 Run（honor -w）
    pub async fn handle_cli(&self) -> anyhow::Result<CliAction>;

    /// updater 下恒可用；prod+Linux 才真正发心跳，否则 no-op
    pub fn spawn_watchdog(&self) -> tokio::task::JoinHandle<()>;
}

/// 宿主把它作为自己 enum 的一个变体（透传），或经 parse_deploy 获取
pub enum DeployCommand {
    Install { dry_run: bool, workspace: Option<String> },
    Update  { force: bool },
    Version,
    Help,
}

pub enum CliAction {
    Handled,                                  // install/update 已执行并 log，调用方退出
    DryRun(String),                           // install --dry-run 的 unit，调用方打印
    Version(String),                          // --version 的版本串，调用方打印
    Help(String),                             // --help 的 deploy 用法段，调用方打印
    Run { workspace: std::path::PathBuf },    // 非 deploy 命令，调用方按服务正常跑
}
```

**透传集成**：宿主拥有顶层 CLI，把 `DeployCommand` 作为一个变体嵌入，未匹配回退自己的命令——库不读全局 argv、不做 stdout/exit。

```rust
// A. 透传组合（推荐，宿主掌控 CLI）
enum AppCmd { Serve, Deploy(DeployCommand) }
let app = match svc.parse_deploy() { Some(c) => AppCmd::Deploy(c), None => AppCmd::Serve };
match app {
    AppCmd::Deploy(c) => match svc.dispatch(c).await? {
        CliAction::DryRun(t) | CliAction::Version(t) | CliAction::Help(t) => println!("{t}"),
        CliAction::Handled => {}
        CliAction::Run { .. } => unreachable!(),
    },
    AppCmd::Serve => { let _wd = svc.spawn_watchdog(); serve(svc.workspace()?).await? }
}

// B. 零配置糖（等价上面整段）
match svc.handle_cli().await? {
    CliAction::Run { workspace } => { let _wd = svc.spawn_watchdog(); serve(workspace).await? }
    CliAction::DryRun(t) | CliAction::Version(t) | CliAction::Help(t) => println!("{t}"),
    CliAction::Handled => {}
}
```

约定（固化；要别的形状就直接用细粒度方法）：优先级 `--version`/`-V` > `--help`/`-h` > `install` > `update`；`install` 收 `--dry-run`/`-n` 与 `-w`/`--workspace <path>`（同时作用于安装与运行时 workspace，保证 `WorkingDirectory` 与 `args::workspace` 一致）；`update` 收 `--force`/`-f`。完整可编译示例见 [`examples/linux_service.rs`](../examples/linux_service.rs)。

### 2. 自更新 `update.rs`

`UpdateConfig` 在原有基础上新增 `install_dir`：

```rust
impl UpdateConfig {
    pub fn new(repo_owner, repo_name, current_version) -> Self;
    pub fn bin_name(self, name: impl Into<String>) -> Self;
    pub fn extra_bins<I, S>(self, bins: I) -> Self;
    pub fn force(self, force: bool) -> Self;
    pub fn target_triple(self, target: impl Into<String>) -> Self;
    /// 安装目录，默认 = 当前可执行文件所在目录；
    /// 设置后写入该目录（如服务的 ~/.local/bin），运行进程与部署位置可不同。
    pub fn install_dir(self, dir: impl Into<PathBuf>) -> Self;
    pub async fn execute(&self) -> anyhow::Result<UpdateOutcome>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome {
    UpToDate { current: String, latest: String },
    Updated { from: String, to: String, bins: Vec<String> },
}
```

`execute()` 流程不变（client → target triple → release JSON → 版本比较 → 流式下载 `{bin}.update.tmp` → `swap_in_place` 回滚式替换），唯一变化是落盘目录取 `install_dir`（未设则仍为 `current_exe().parent()`）。

### 3. 用户级 systemd 安装 `systemd.rs`

```rust
impl ServiceConfig {
    pub fn new(name: impl Into<String>) -> Self;

    pub fn description(self, desc: impl Into<String>) -> Self;
    pub fn exec_args(self, args: impl Into<String>) -> Self;   // {workspace} 渲染时替换
    pub fn binaries<I, S>(self, bins: I) -> Self;
    pub fn user(self, user: impl Into<String>) -> Self;        // 默认当前登录用户
    pub fn user_home(self, home: impl Into<PathBuf>) -> Self;  // 默认当前用户 home
    pub fn bin_dir(self, dir: impl Into<PathBuf>) -> Self;     // 默认 <home>/.local/bin
    pub fn workspace(self, path: impl Into<PathBuf>) -> Self;  // 默认 <home>/.config/<name>
    pub fn restart_sec(self, secs: u32) -> Self;
    pub fn watchdog_sec(self, secs: u32) -> Self;              // 开启 Type=notify + WatchdogSec

    pub fn bin_dir_path(&self) -> anyhow::Result<PathBuf>;
    pub fn workspace_path(&self) -> anyhow::Result<PathBuf>;

    /// 渲染 unit 文件内容（全平台可用，适合 --dry-run）。返回 Result。
    pub fn generate_unit(&self) -> anyhow::Result<String>;

    /// 完整安装（用户级，免 root）；仅 Linux + `updater` 真正实装。
    pub fn install(&self) -> anyhow::Result<()>;
}
```

用户 / home / bin_dir / workspace **惰性解析**：

- `user`：显式 > `$USER` / `$USERNAME` > 报错（仅用于 `loginctl enable-linger` 目标）
- `home`：显式 > `home::home_dir()` > `/home/<user>`
- `bin_dir`：显式 > `<home>/.local/bin`
- `workspace`：显式 > `<home>/.config/<name>`

#### `install()` 内部流程（Linux + `updater`，全程免 root）

1. 解析 user/home/bin_dir/workspace。
2. **检测重复安装并提醒（不阻断）**：PATH 上存在同名二进制于非目标目录 → `log::warn!`（提示 `~/.local/bin` 须排在 PATH 前面，否则旧副本会遮蔽新的）；`/etc/systemd/system/<name>.service` 存在 → `log::warn!`（旧系统单元可能冲突，建议 disable 移除）。
3. `create_dir_all(bin_dir)`，从当前可执行文件目录复制 `binaries` 到 `bin_dir`（src==dest 跳过 copy），`chmod 755`。
4. `create_dir_all(workspace)`。
5. 写 `~/.config/systemd/user/{name}.service`。
6. `systemctl --user daemon-reload` && `systemctl --user enable {name}`。
7. `loginctl enable-linger {user}`（让服务脱离登录会话/开机自启）；**失败仅 `log::warn!` 不致命**——锁定主机上可能需管理员补一次。

#### 生成的 unit 模板（用户单元）

```ini
[Unit]
Description={description}
After=network.target

[Service]
Type={simple|notify}                          # 设了 watchdog_sec 则 notify
ExecStart={bin_dir}/{name} {exec_args}        # {workspace} 已替换；无 exec_args 时无尾随空格
Restart=on-failure
RestartSec={restart_sec}
WatchdogSec={secs}                            # 仅 watchdog_sec 设置时出现
WorkingDirectory={workspace}

[Install]
WantedBy=default.target
```

要点：用户单元**不写 `User=`/`Group=`**（`systemctl --user` 必以归属用户运行，写了反而报错）；`[Install]` 用 `default.target`（用户总线下 `multi-user.target` 的对应物）。

`watchdog_sec` 修复了原有隐患：`util_daemon` 一直发送 `sd_notify(Watchdog)`，但旧 unit 从不设 `Type=notify` / `WatchdogSec`，心跳实际是空转。

---

## 4. 接入方使用示例

完整可编译示例：[`examples/linux_service.rs`](../examples/linux_service.rs)
（`cargo run --example linux_service --features updater [-- ...]`），覆盖透传组合（Style A）与 `handle_cli` 零配置（Style B）两种风格——API 形状见上文 §1 的两段示例，不在此重复以免文档与代码漂移。

运行期一致性：服务以登录用户身份跑，`custom_utils::args::workspace(&arg, "alarm-server")` 解析得 `~/.config/alarm-server`，与 `LinuxService::workspace()` / unit 的 `WorkingDirectory` 一致——这是「配置统一通过 `arg::workspace`」的落点。

---

## 5. Release 产物命名约定

资产匹配规则为「文件名 **同时包含** 二进制名与 target triple」：

```
{bin_name}-{target}{ext}
{bin_name}_{target}{ext}
```

| 产物示例 | 说明 |
|---------|------|
| `alarm-cli-x86_64-pc-windows-msvc.exe` | Windows x86_64 |
| `alarm-cli-aarch64-unknown-linux-gnu` | Linux ARM64 |
| `alarm-server-aarch64-unknown-linux-gnu` | Linux ARM64 |

自动探测的 target triple：`x86_64-pc-windows-msvc` / `x86_64-unknown-linux-gnu` / `aarch64-unknown-linux-gnu`，其他平台用 `.target_triple(..)` 显式指定。接入项目的 `.github/workflows/release.yml` 需保持一致。
