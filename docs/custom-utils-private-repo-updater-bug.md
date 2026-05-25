# Bug: custom-utils 0.14.1 updater 无法从私有 GitHub 仓库下载 release 资产

> **此文档独立、自包含**，新 session 无需读取本会话上下文即可动工。
> 写入时间：2026-05-25。发现于 zero v0.3.5 部署 g10 真机时。

- **仓库**：`D:\git\custom-utils`（git tag v0.14.1）
- **模块**：`src/util_updater/update.rs`
- **影响范围**：所有用 `UpdateConfig::execute()` 自更新且 GitHub 仓库为 PRIVATE 的项目（`jm-observer/zero` 是其中一个）

## 现象

zero（私有仓库 `jm-observer/zero`）配置好 `GITHUB_TOKEN` 环境变量后，IM 命令 `/update` 报：

```
更新失败: Failed to download asset for 'zero'
```

不是 metadata 阶段失败（metadata 拉取走的是 `https://api.github.com/...` API，由 `build_client()` 注入了 `Authorization: Bearer $GITHUB_TOKEN`，私有仓库能拿到）。挂在**下载 asset 阶段**。

## 根因

`fn find_asset_url` (update.rs:243-258) 返回 `asset["browser_download_url"]`，这是一个 `https://github.com/<owner>/<repo>/releases/download/<tag>/<name>` 形式的 URL。

对私有仓库，访问此 URL 会 **HTTP 302 重定向到 `https://objects.githubusercontent.com/...`**（GitHub CDN）。

**`reqwest` 的默认 redirect policy 在跨主机重定向时会丢弃 `Authorization` header**（安全策略，避免把凭证泄露到第三方域名）。结果：CDN 收到无凭证请求 → **HTTP 404**（同样 GitHub 对私有资源故意返回 404 而非 403，避免暴露存在性）。

`download_to` 拿到 404 → `error_for_status()` 抛错 → 上层 `with_context("Failed to download asset for '{bin}'")` 包装成最终错误。

## 实验证据（在 g10 / aarch64-linux 上跑）

| 请求方式 | 结果 |
|---------|------|
| `curl -L -H "Authorization: Bearer $TOKEN" https://github.com/jm-observer/zero/releases/download/v0.3.5/zero_aarch64-unknown-linux-gnu` | **HTTP 404, size=9**（同 updater 行为） |
| `curl -L -H "Authorization: Bearer $TOKEN" -H "Accept: application/octet-stream" https://api.github.com/repos/jm-observer/zero/releases/assets/429165030` | **HTTP 200, size=36675920** ✅ |

第二种方式（GitHub Releases API 的 assets 端点 + `Accept: application/octet-stream`）对公开仓库和私有仓库都返回二进制内容（公开仓库下行为不变，向后兼容）。

## 推荐修复

### 改动 1：`find_asset_url` 返回 API URL（不是 `browser_download_url`）

`src/util_updater/update.rs:254`

```rust
// 改前
asset["browser_download_url"]
    .as_str()
    .map(str::to_string)
    .ok_or_else(|| anyhow!("Asset is missing browser_download_url"))

// 改后
asset["url"]
    .as_str()
    .map(str::to_string)
    .ok_or_else(|| anyhow!("Asset is missing API url"))
```

`asset["url"]` 是 `https://api.github.com/repos/<owner>/<repo>/releases/assets/<id>` 形式，重定向到 CDN 时也会**带 token**（因为是同 `api.github.com` 主机层 → S3 跨主机时，GitHub 主动签一个 short-lived signed URL，不依赖 caller 的 Authorization）。

### 改动 2：`download_to` 设置 `Accept: application/octet-stream`

`src/util_updater/update.rs:261-281`

```rust
async fn download_to(client: &Client, url: &str, path: &Path) -> Result<()> {
    use futures_util::StreamExt;
    use reqwest::header::{ACCEPT, HeaderValue};

    let resp = client
        .get(url)
        .header(ACCEPT, HeaderValue::from_static("application/octet-stream"))
        .send()
        .await
        .context("Failed to request binary download")?
        .error_for_status()
        .context("Download request returned an error status")?;
    // ...其余不变
}
```

API assets 端点行为：

- `Accept: application/vnd.github+json`（`build_client()` 的默认值）→ 返回 asset metadata JSON
- `Accept: application/octet-stream` → 302 重定向到带签名 URL 的 CDN，跟随后拿到二进制

所以 `download_to` 必须**显式覆盖**默认的 vnd.github+json，否则会下到 JSON 而不是二进制。

## 兼容性

| 场景 | 改后行为 |
|------|---------|
| 公开仓库 + 无 token | API endpoint 仍可用（GitHub 对公开仓库 unauthenticated API 给 60/h rate limit，足够偶尔升级） |
| 公开仓库 + 有 token | API endpoint 走 token 认证，rate limit 提到 5000/h |
| 私有仓库 + 无 token | metadata 阶段就 404，与改前等价（不可用，但本来也不可用） |
| **私有仓库 + 有 token** | **API endpoint + 302 签名 URL，可下载 ✅（修复目标）** |

## 测试要求

### 单元测试新增 / 调整（`#[cfg(test)] mod tests`）

`asset_matching_picks_bin_and_target` 当前用的 fixture：

```rust
let assets = json!([
    { "name": "alarm-server-x86_64-pc-windows-msvc.exe",
      "browser_download_url": "https://example.com/server-win" },
    ...
]);
```

改后 fixture 必须**同时**含 `"url"` 字段（API URL），且测试 assertion 改为断言返回 `"url"` 的值而非 `"browser_download_url"`。

新增一个测试 `download_to_sets_octet_stream_accept`：用 `mockito` / `wiremock` 起一个 HTTP server，断言收到的请求 `Accept` header 是 `application/octet-stream`，且 body 写入文件后字节与 mock 响应一致。

### 集成验证（手动）

升级后用一个私有仓库的 `UpdateConfig::execute()` 跑一遍，确认能 update 成功。或在 zero 仓直接重新发一个 tag（如 v0.3.6），在 g10 上发 `/update` 看是否能自动升级（不再需要本次的 scp 绕过）。

## 发布联动

参考 `MEMORY.md` 中的 [[project_zero_nova_custom_utils_coupling]] 约束（zero ↔ zero-nova ↔ custom-utils 版本耦合）：

1. **custom-utils 仓**：改完，bump `0.14.1` → **`0.14.2`**，发新 git tag `v0.14.2`，push 远端
2. **zero-nova 仓**（仅当 zero-nova 自己也依赖 custom-utils 时）：
   - `Cargo.toml` 把 `custom-utils = "0.14.1"` 改成 `"0.14.2"`
   - 发新 nova tag（如 `v0.3.16`），push 远端
   - 先 `grep -r custom-utils D:\git\zero-nova\Cargo.toml`，**若 zero-nova 未直接依赖 custom-utils，本步跳过**
3. **zero 仓**：
   - `Cargo.toml` 把 `zero-nova` 改指新 tag（若步骤 2 跳过则不动）；workspace 里如直接依赖 custom-utils，把版本改成 `0.14.2`
   - `cargo update -p custom-utils` (+ `-p nova-agent` if step 2 happened)
   - `cargo make check`（fmt + clippy + test 全通过）
   - 提交 commit，bump 一个新 tag（如 `v0.3.6`），push（含 tag）触发 Release CI
4. **g10 真机验证**：
   - 确保 `~/.config/systemd/user/zero.service.d/override.conf` 仍含 `GITHUB_TOKEN`（先前注入未删）
   - IM 发 `/update`，期望主 Agent 回包 `已更新 X.Y.Z -> 0.3.6，正在重启...`，systemd 拉起后服务保持 active
   - 若仍失败，看 `journalctl --user -u zero --since='2 min ago'` 拿完整 err

## 当前的临时绕过（v0.3.5 已用此法部署 g10）

在 g10 上手动用 curl 调 API endpoint 拉资产，atomic mv 替换 `~/.local/bin/zero` 后 `systemctl --user restart zero`。详见 zero 仓本次会话 git 历史（`1cde411` 之后的部署操作未入库，仅为一次性手工动作）。本次 v0.3.5 已经在跑（sha256 `2767067a307304d14349557007d53cc7204f611642db2331a1fd9fbe7d3ed95b` 与 release 资产一致）。

## 给新 session 的简短开工 prompt

```
请修复 D:\git\custom-utils 仓的 updater bug：私有 GitHub 仓库下，
UpdateConfig::execute() 在下载 asset 阶段失败（HTTP 404）。完整根因、
实验证据、修复 patch 草稿、测试要求、发布联动详见 zero 仓的
docs/2026-05-25-flag-triggers-review/custom-utils-private-repo-updater-bug.md
（自包含，无需读其他文档）。
```
