<p align="center">
  <img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="将网站变成 AI 可用的工具。" width="100%" />
</p>

<p align="center">
  <strong>将任何网站转化为可供 AI 智能体和终端直接使用的带类型 CLI 与 MCP 工具。</strong><br/>
  <sub>直接复用真实浏览器登录态，无需担心向大模型泄露 Cookie 或触发反爬拦截，轻松实现读取、搜索、创建、编辑与自动化。</sub>
</p>

<p align="center">
  <a href="https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0">下载 (Windows · macOS · Linux)</a> ·
  <a href="https://github.com/Hybirdss/Hycli/issues">在 GitHub 上获取帮助</a> ·
  <a href="#try-it-in-30-seconds">30 秒快速体验</a> ·
  <a href="#get-started">开始使用</a> ·
  <a href="#what-your-ai-can-do">AI 能做什么</a> ·
  <a href="#common-concerns">常见顾虑与解决方式</a> ·
  <a href="#use-hycli-with-your-ai">与 AI 配合使用</a>
</p>

<details>
<summary>选择阅读语言 · 20 种语言</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

只需提供任意网站地址和你的目标。三个 AI 工作单元会分析文档、前端 JS 包和真实浏览器网络流量，构建包含读取与写入操作的带类型 CLI 和 MCP 工具集——且在准备过程中绝不会向你的真实账号发送任何写入请求。

<p align="center">
  <img src="../../docs/images/dashboard.png" alt="Hycli 仪表盘，显示网站卡片、可用操作和任务进度。" width="100%" />
</p>
<p align="center"><sub>在一个本地仪表盘和 CLI 中集中管理你的网站、账号、写入权限与执行结果。</sub></p>

<a id="try-it-in-30-seconds"></a>
## 30 秒快速体验

```sh
# 1. 指定任意网站并描述你的目标（同时发掘读取与写入操作，准备期间零真实写入）
hycli prepare https://your-website.example --intent "搜索帖子、保存草稿并发布新文章"

# 2. 一览所有已准备好的读取、写入命令及推荐的多步工作流
hycli describe your-website

# 3. 立即执行读取或搜索操作
hycli your-website search-posts --query "发布说明"

# 4. 需要创建、编辑或删除时，为该 CLI 一次性开启写入权限即可直接运行
hycli writes your-website allow
hycli your-website create-draft --title "来自 Hycli 的问候" --body "直接从终端自动创建。"
```

或者在 **Claude Code、Cursor、Codex、Windsurf** 中一次性添加 `"command": "hycli", "args": ["mcp"]`，然后用自然语言对智能体说：
> *“用 Hycli 准备 `https://...` 的 CLI，告诉我有哪些可用操作，然后帮我写一篇新草稿。”*

<a id="get-started"></a>
## 开始使用

Hycli 将网站引擎与本地仪表盘（`http://127.0.0.1:4318`）打包在单个原生可执行文件中。运行发行包无需安装 Rust 或 Node.js。

### 下载与运行 (Windows, macOS, Linux)

前往 **[GitHub Releases (v0.1.0)](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)** 下载适合你操作系统的原生安装包或便携压缩包：

| 操作系统 | 架构 | 原生安装包 | 便携压缩包 | 启动方式 |
| --- | --- | --- | --- | --- |
| **Windows 10/11** | x64 及 ARM64 | `hycli-0.1.0-win32-*-setup.exe` | `hycli-0.1.0-win32-*.tar.gz` | 运行 `*-setup.exe`（无需管理员权限，自动添加至用户 `PATH` 和开始菜单），然后点击 **Hycli** 图标或在 PowerShell 中运行 `hycli open` |
| **macOS 13+** | Apple Silicon (`arm64`) 及 Intel (`x64`) | `hycli-0.1.0-darwin-*.dmg` | `hycli-0.1.0-darwin-*.tar.gz` | 打开 `.dmg`，将 **Hycli.app** 拖入 **Applications** 后运行（自动链接 `~/.local/bin/hycli`） |
| **Linux** | x64 及 ARM64 | `hycli-0.1.0-linux-*.deb` | `hycli-0.1.0-linux-*.tar.gz` | 安装 `.deb`（`sudo apt install ./hycli-*.deb`）或解压 `.tar.gz` 后运行 `./install.sh`，随后执行 `hycli open` |

<details>
<summary><strong>终端便携包安装与从源码构建</strong></summary>

**Linux / macOS 便携包安装：**

```sh
sha256sum -c hycli-0.1.0-*.tar.gz.sha256   # macOS 请使用 shasum -a 256 -c
tar -xzf hycli-0.1.0-*.tar.gz
cd hycli-0.1.0-*
./install.sh
hycli open
```

**Windows PowerShell 便携包安装：**

```powershell
tar -xzf hycli-0.1.0-win32-x64.tar.gz
cd hycli-0.1.0-win32-x64
powershell -ExecutionPolicy Bypass -File .\install.ps1
hycli open
```

**从源码构建 (Linux, macOS, Windows)：**

安装[构建前置依赖](../BUILD.md)后执行以下命令：

```sh
node scripts/build.mjs
./dist/hycli open
```

如需生成当前操作系统的原生安装包（`.exe`、`.dmg`、`.deb`），请运行 `node scripts/package-desktop.mjs`。详见[构建与打包说明](../BUILD.md)和[发布验证](../RELEASING.md)。

</details>

### 连接第一个网站

1. 运行 `hycli open`（或点击 **Hycli** 应用图标）打开本地仪表盘（`http://127.0.0.1:4318`）。
2. 在 **AI 连接**中连接 OpenAI、Anthropic Claude、xAI、Z.ai 或本地 ChatGPT (Codex) 登录。
3. 对于需要登录的网站，点击 **账号 → 查找我的登录状态** 将浏览器会话导入本地加密保险库。
4. 输入网站地址和目标以生成工具，或从 **编程代理** 复制 MCP 配置并粘贴到你的 AI 助手中。

<a id="what-your-ai-can-do"></a>
## AI 能做什么

Hycli 将网站的各类真实接口转换为自描述的 CLI 子命令与 MCP 工具：

| 网站接口 | 准备好的能力 |
| --- | --- |
| REST 与 OpenAPI | 基于 JSON/YAML 模式和 API 文档的带类型读取与写入请求 |
| GraphQL | 解析后的查询与变更（mutation）文档及带类型的 JSON 变量 |
| HTML 与 JS 渲染页面 | 基于所观察选择器的列表、链接、分页提取，以及通过本地 Chromium 引擎读取渲染后的 GET 页面 |
| JSON 与 URL 编码表单 | 带文档说明的输入项、默认值及绑定账号的表单请求（`POST`、`PUT`、`PATCH`、`DELETE`） |

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="三位工作者。一只忙得团团转的小鸟。" width="100%" />
</p>
<p align="center"><sub>三位工作者。一只忙得团团转的小鸟。</sub></p>

| 操作类型 | 执行行为 |
| --- | --- |
| **读取或搜索** | 只要有证据支持且被归类为安全读取，即可立即执行。 |
| **创建、发送、编辑或删除** | 在准备过程中会被发掘并合成到 CLI 规范中，但在准备和检查期间绝不会执行。每个网站 CLI 默认关闭写入（`writes: false`）。AI 智能体会**按每个 CLI 仅询问你一次**是否允许写入（`hycli writes SITE allow` 或 MCP `hycli_writes`），开启后即可按你的指令直接执行写入操作。 |
| **影响不明确的操作** | 在发送请求前暂缓并交由审核。 |
| **身份验证挑战或速率限制** | 立即切换传输策略（HTTP → 页面内 `window.fetch` → 隐身 Chromium）或安全暂停。 |

支持的请求形式与提取规则请参阅[操作规范](../SITESPEC.md)。

<a id="common-concerns"></a>
## 常见顾虑与 Hycli 的解决方式

将 AI 智能体连接到真实网站和已登录账号时，人们最常担心的几个问题以及 Hycli 在架构上的应对方式：

### 1. “AI 会不会在我的账号里误发帖、乱修改或误删数据？”
- **创建与检查 CLI 期间零真实写入：** 在执行 `hycli prepare`、`hycli request`、`hycli recon`、`hycli check` 准备或检查网站时，Hycli 会发掘并编译读取和写入操作，但**绝不会向服务器发送任何真实写入请求**。即使是伪装成读取的变更请求（如包含 `/delete` 等变更路径的 `GET`、GraphQL `mutation`、HTML `<form method="post">` 提交），在验证阶段也会被严格拦截。
- **每个 CLI 首次写入前的一次性用户确认：** 每个新生成的网站 CLI 默认都处于关闭写入状态（`writes: false`）。当智能体首次需要在某个 CLI 上执行发帖、编辑或删除等写入操作时，会**针对该 CLI 询问你一次**：*“是否允许 `<site>` 执行写入操作？”* 只有你确认并开启 `hycli writes <site> allow`（或 MCP `hycli_writes`）后，写入操作才会直接执行；你随时可以通过 `hycli writes <site> deny` 或仪表盘中的 **允许更改** 开关收回权限。
- **写入失败绝不自动重试：** 失败或中断的写入请求绝不会自动重试。如果在发送变更请求后连接中断，Hycli 会返回 `action_outcome_unknown`，让你先检查网站实际状态，避免重复下单或重复发帖。

### 2. “Hycli 会把我的登录 Cookie、令牌或 API 密钥发给 AI 模型（LLM）吗？”
- **秘密信息零进入 AI 提示词：** 浏览器 Cookie、`localStorage` / `sessionStorage` 令牌和 API 密钥严格保存在本地加密保险库中（通过 Windows 凭据管理器、macOS 钥匙串、Linux Secret Service 加密，或限制为 `0600` 当前用户专享权限）。
- **符号化连接配方（`ConnectionRecipe`）：** AI 模型只能看到账号标签、存储键名称和脱敏后的响应结构。模型只构建引用本地键名的符号化 `ConnectionRecipe`，真正的秘密值由 Hycli 在执行时于本地注入，并自动从所有输出与日志中抹除。
- **按站点隔离的只读会话导入：** 导入浏览器会话时仅通过临时只读快照读取目标域名的 Cookie 和存储，绝不削弱浏览器安全或导出其他网站的 Cookie。

### 3. “抓取操作会不会触发验证码（CAPTCHA）或导致账号被封禁？”
- **绝不模糊测试（Fuzzing）或暴力扫描：** Hycli 从不猜测端点字典或对线上服务器进行模糊测试，而是仅从链接文档、OpenAPI/GraphQL 模式、静态 JS 包和观察到的真实浏览器网络结构中学习 API。
- **按站点与账号限流串行发送：** 请求按网站和账号串行排队并限制速率，严格遵守 `Retry-After` 响应头，并限制读取重试次数和响应体大小。
- **策略熔断器与真实浏览器上下文执行：** 当普通 HTTP 请求遇到 403/429 或反爬验证（如 Cloudflare、Akamai Bot Manager）时，Hycli 绝不会死循环重试被拦截的请求。策略熔断器会立即切换传输方式——在已认证的真实浏览器上下文中执行 `window.fetch(..., { credentials: 'include' })` 或切换至本地隐身 Chromium 引擎——或安全暂停以保护你的账号。

### 4. “重定向会不会泄露我的会话，或者分享规范文件会不会暴露我电脑上的路径？”
- **严格的源站绑定（Origin Binding）：** 身份验证请求头和 Cookie 严格锁定在目标源站，发生跨源重定向时立即剥离。私有网段和本地回环地址默认拦截，除非显式传入 `--allow-local`。
- **可移植的规范文件：** 本地浏览器可执行文件路径与配置文件路径仅保留在本机状态中，绝不会写入可移植的 `SiteSpec` 文件。

### 5. “当登录过期时，AI 会不会不断重试而白白浪费 Token？”
- **无需 AI 的确定性健康检查（`hycli check`）：** 执行 `hycli check`（及 MCP `hycli_check`）时无需调用大模型即可重放已验证的读取链，准确区分 `signed_out`（登录过期，需重连账号）、`site_changed`（网站结构变更，需修复规范）、`blocked` 和 `unreachable`。

<a id="ai-connections"></a>
## AI 连接

| 服务提供商 | 认证方式 |
| --- | --- |
| ChatGPT · API 密钥 | OpenAI API 密钥 |
| Claude | Anthropic API 密钥 |
| xAI | xAI API 密钥 |
| Z.ai / Z.ai Coding Plan | Z.ai API 密钥 |
| ChatGPT · 登录 | 通过官方 Codex app-server 协议管理的本地 ChatGPT 登录 |

Codex 登录连接在禁用文件、Shell、浏览器和外部 MCP 执行的临时线程中运行；Hycli 从不读取或复制 Codex OAuth 令牌。

<a id="use-hycli-with-your-ai"></a>
## 将 Hycli 与 AI 配合使用

将 Hycli 添加到你的 MCP 客户端（**编程代理 → 查看连接设置** 会显示当前操作系统的准确路径）：

```json
{
  "mcpServers": {
    "hycli": {
      "command": "hycli",
      "args": ["mcp"]
    }
  }
}
```

如果只想暴露已安装的网站工具，请使用 `hycli mcp --sites-only`；如需限定到单个网站，请添加 `--only-site SITE`。MCP 服务器提供 `hycli_prepare`、`hycli_writes`（按每个 CLI 首次询问你后开启写入）、`hycli_run`、`hycli_check`、`hycli_result` 和 `hycli_activity`，且绝不提供任何读取凭据的工具。

在终端中也可以使用完全相同的工作流：

```sh
hycli prepare https://your-website.example --intent "查找已保存的参考资料"
hycli request SITE "添加正文编辑、草稿保存和文章发布功能"
hycli describe SITE
hycli SITE ACTION --help
hycli writes SITE allow
hycli SITE ACTION --query "design systems"
hycli check SITE
```

[智能体指南](../../agent/AGENTS.md) · [Hycli 技能](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## 浏览器登录

Hycli 会在 Windows、macOS 和 Linux 上发现本地 Chrome、Chromium、Edge、Brave、Vivaldi、Arc、Whale 和 Firefox 配置文件（以及已安装的本地隐身 Chromium 引擎），仅将所请求网站的会话导入本地加密保险库。指向同一个已验证网站账号的多个配置文件会自动合并去重。

如果会话受到 Windows App-Bound 加密、隐私容器或未导入浏览器的保护，可在 **账号 → 连接账号** 中通过浏览器登录、连接[浏览器辅助扩展](../../browser-companion/guide.html)，或在仪表盘中导入 JSON/Netscape Cookie 文件（切勿将 Cookie 内容直接粘贴到 AI 聊天窗口中）。

<a id="languages"></a>
## 语言

仪表盘和 AI 生成的操作说明支持本 README 顶部链接的 20 种语言，包括从右向左书写的阿拉伯语（RTL）。

<a id="local-data"></a>
## 本地数据与请求限制

网站定义、加密凭据和活动记录保存在操作系统的本地数据目录中（Windows 为 `%LOCALAPPDATA%\hycli`，macOS 为 `~/Library/Application Support/hycli`，Linux 为 `~/.local/share/hycli`，或通过 `HYCLI_DATA_DIR` 指定）。你可以随时在 **设置** 中查看当前路径与密钥环加密状态。

<a id="contributing"></a>
## 开发与验证

```sh
npm --prefix dashboard ci
npm --prefix dashboard run check
npm --prefix dashboard run build
cargo test --all-targets --locked
cargo build --locked --bin hycli --example dashboard_fixture
node scripts/test-e2e.mjs --full
```

所有测试均使用合成的本地网站与测试夹具运行——切勿为了测试而在真实的第三方服务账号上执行写入操作。另请参阅[视觉素材来源](../images/README.md)。

## 许可证与预期用途

Hycli 采用 [Apache-2.0](../../LICENSE) 许可证发布。请仅在你有权访问的网站和账号上使用。本软件按“原样”提供，不附带任何明示或暗示的保证；作者和贡献者不对因使用本软件而导致的任何损失、账号受限或封禁承担责任。

## Star History

<a href="https://www.star-history.com/#Hybirdss/Hycli&Date">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date&theme=dark" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
 </picture>
</a>
