# util_updater 模块设计文档

`custom-utils` 的 `util_updater` 模块，为 CLI 项目提供 **GitHub 自更新** + **systemd 服务安装** 两项能力。公开路径为 `custom_utils::updater`。

> 实现说明：本模块全程使用 **async `reqwest`**，未引入 `self_update`（与项目「禁止阻塞 API」规则一致）；root 检测用 `id -u` 而非 `libc`，因此无额外原生依赖、无 `unsafe`。

---

## 依赖与 feature

模块依赖工作区已有的 `anyhow` / `reqwest` / `serde_json` / `futures-util` / `tokio`，无新增第三方 crate。

```toml
[features]
updater = ["tokio"]                  # 自更新（跨平台）
updater-systemd = ["updater"]        # 额外启用 systemd 实装（仅 Linux 生效）
```

接入方按需选择：

```toml
# 只要自更新（Windows / 跨平台）
custom-utils = { version = "0.11", default-features = false, features = ["updater"] }

# 需要 systemd 安装（Linux 服务）
custom-utils = { version = "0.11", default-features = false, features = ["updater-systemd"] }
```

---

## 模块结构

```
src/util_updater/
├── mod.rs       # pub use ServiceConfig / UpdateConfig / UpdateOutcome
├── update.rs    # GitHub Release 自更新（async）
└── systemd.rs   # systemd 服务安装
```

---

## API 设计

### 1. 自更新 `update.rs`

```rust
pub struct UpdateConfig { /* 私有字段，builder 构造 */ }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome {
    /// 最新版本不比当前版本新，未做任何改动
    UpToDate { current: String, latest: String },
    /// 二进制已被替换
    Updated { from: String, to: String, bins: Vec<String> },
}

impl UpdateConfig {
    pub fn new(
        repo_owner: impl Into<String>,
        repo_name: impl Into<String>,
        current_version: impl Into<String>,
    ) -> Self;

    /// 主二进制名，默认取当前可执行文件名
    pub fn bin_name(self, name: impl Into<String>) -> Self;
    /// 同 release 中一起更新的其他二进制（同目录）
    pub fn extra_bins<I, S>(self, bins: I) -> Self
        where I: IntoIterator<Item = S>, S: Into<String>;
    /// 即使不是更新版也强制更新
    pub fn force(self, force: bool) -> Self;
    /// 覆盖自动探测的 target triple（资产匹配用）
    pub fn target_triple(self, target: impl Into<String>) -> Self;

    /// 执行更新；非 force 且无新版本时返回 UpToDate
    pub async fn execute(&self) -> anyhow::Result<UpdateOutcome>;
}
```

#### 使用方式

```rust
let outcome = custom_utils::updater::UpdateConfig::new(
        "jm-observer", "timer-util", env!("CARGO_PKG_VERSION"))
    .bin_name("alarm-cli")
    .extra_bins(["alarm-server"])
    .force(args.force)
    .execute()
    .await?;
```

#### 内部要点

`execute()` 流程：

1. 构建 `reqwest::Client`；若环境变量 `GITHUB_TOKEN` 存在，附加 `Authorization: Bearer`（提升限额、支持私有仓库）。
2. 确定 target triple（自动探测或 `target_triple()` 覆盖）。
3. 请求 `https://api.github.com/repos/{owner}/{repo}/releases/latest`，解析 `tag_name` 与 `assets`。
4. 版本比较：去掉前导 `v`、忽略 `-rc` / `+build` 后缀做点分数字比较；非 `force` 且不更新则返回 `UpToDate`。
5. 主二进制 + 每个 `extra_bins`：在 assets 中找 **名称同时包含二进制名与 target** 的资产，流式下载到 `{bin}.update.tmp`，再就地替换。
6. 返回 `Updated { from, to, bins }`。

替换策略（`swap_in_place`）：目标存在则先 `rename` 为 `<dest>.bak`，新文件 `rename` 就位；失败则回滚 `.bak`；Unix 下 `chmod 755`。目标不存在（首次安装某 extra bin）则直接就位、不产生 `.bak`。

---

### 2. systemd 安装 `systemd.rs`

```rust
pub struct ServiceConfig { /* 私有字段，builder 构造 */ }

impl ServiceConfig {
    pub fn new(name: impl Into<String>) -> Self;          // user/workspace/binaries 默认由 name 派生

    pub fn description(self, desc: impl Into<String>) -> Self;
    pub fn exec_args(self, args: impl Into<String>) -> Self;   // {workspace} 占位符渲染时替换
    pub fn binaries<I, S>(self, bins: I) -> Self
        where I: IntoIterator<Item = S>, S: Into<String>;
    pub fn user(self, user: impl Into<String>) -> Self;
    pub fn workspace(self, path: impl Into<String>) -> Self;
    pub fn restart_sec(self, secs: u32) -> Self;

    /// 渲染 unit 文件内容（全平台可用，适合 --dry-run）
    pub fn generate_unit(&self) -> String;

    /// 执行完整安装；仅 Linux + `updater-systemd` feature 下真正实装，
    /// 否则返回错误（提示改用 generate_unit 预览）。需要 root。
    pub fn install(&self) -> anyhow::Result<()>;
}
```

`new(name)` 的默认值：`description = "{name} service"`、`binaries = [name]`、`user = name`、`workspace = /etc/{name}`、`restart_sec = 5`。

#### 使用方式

```rust
let svc = custom_utils::updater::ServiceConfig::new("alarm-server")
    .description("Alarm Server - Recurring alarm scheduler")
    .exec_args("-w {workspace}")
    .binaries(["alarm-server", "alarm-cli"])
    .user(&args.user)
    .workspace(&args.workspace);

if dry_run {
    println!("{}", svc.generate_unit());
} else {
    svc.install()?;
}
```

#### `install()` 内部流程（Linux + `updater-systemd`）

1. `id -u` 检查是否 root，非 root 报错提示 `sudo`。
2. 从当前可执行文件所在目录复制 `binaries` 到 `/usr/local/bin/`，`chmod 755`。
3. 读 `/etc/passwd` 判断用户是否存在；不存在则 `useradd --system --no-create-home --shell /usr/sbin/nologin {user}`。
4. `create_dir_all({workspace})` 并 `chown -R {user}:{user} {workspace}`。
5. 写 `/etc/systemd/system/{name}.service`（内容来自 `generate_unit()`）。
6. `systemctl daemon-reload` && `systemctl enable {name}`。

#### 生成的 unit 模板

```ini
[Unit]
Description={description}
After=network.target

[Service]
Type=simple
User={user}
Group={user}
ExecStart=/usr/local/bin/{name} {exec_args}   # {workspace} 已替换为实际路径；无 exec_args 时无尾随空格
Restart=on-failure
RestartSec={restart_sec}
WorkingDirectory={workspace}

[Install]
WantedBy=multi-user.target
```

---

## 3. 接入方使用示例（完整）

```rust
// 项目的 cli.rs

#[derive(Subcommand)]
enum Commands {
    /// 更新到最新版本
    Update {
        #[arg(long)]
        force: bool,
    },
    /// 安装为 systemd 服务
    Install {
        #[arg(long, default_value = "/etc/alarm-server")]
        workspace: String,
        #[arg(long, default_value = "alarm-server")]
        user: String,
        #[arg(long)]
        dry_run: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match cli.command {
        Commands::Update { force } => {
            let outcome = custom_utils::updater::UpdateConfig::new(
                    "jm-observer", "timer-util", env!("CARGO_PKG_VERSION"))
                .bin_name("alarm-cli")
                .extra_bins(["alarm-server"])
                .force(force)
                .execute()
                .await?;
            println!("{outcome:?}");
        }
        Commands::Install { workspace, user, dry_run } => {
            let svc = custom_utils::updater::ServiceConfig::new("alarm-server")
                .description("Alarm Server")
                .exec_args("-w {workspace}")
                .binaries(["alarm-server", "alarm-cli"])
                .user(&user)
                .workspace(&workspace);

            if dry_run {
                println!("{}", svc.generate_unit());
            } else {
                svc.install()?;
            }
        }
    }
    Ok(())
}
```

---

## 4. Release 产物命名约定

GitHub Release 资产匹配规则为「文件名 **同时包含** 二进制名与 target triple」，因此下列命名均可被识别：

```
{bin_name}-{target}{ext}
{bin_name}_{target}{ext}
```

| 产物示例 | 说明 |
|---------|------|
| `alarm-cli-x86_64-pc-windows-msvc.exe` | Windows x86_64 |
| `alarm-cli-aarch64-unknown-linux-gnu` | Linux ARM64 |
| `alarm-server-x86_64-pc-windows-msvc.exe` | Windows x86_64 |
| `alarm-server-aarch64-unknown-linux-gnu` | Linux ARM64 |

自动探测的 target triple：`x86_64-pc-windows-msvc` / `x86_64-unknown-linux-gnu` / `aarch64-unknown-linux-gnu`，其他平台需用 `.target_triple(..)` 显式指定。接入项目的 `.github/workflows/release.yml` 需保持一致。
