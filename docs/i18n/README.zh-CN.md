<p align="center">
  <img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="将网站变成 AI 可用的工具。" width="100%" />
</p>

<p align="center">
  <strong>自动化任何网站。</strong>
</p>

<p align="center">
  <a href="#get-started">开始使用</a> ·
  <a href="#what-your-ai-can-do">AI 能做什么</a> ·
  <a href="#ai-connections">AI 连接</a> ·
  <a href="#use-hycli-with-your-ai">与 AI 配合使用</a>
</p>

<details>
<summary>选择阅读语言 · 20 种语言</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

添加一个网站，Hycli 就会准备好 AI 可用的操作，并解释每项操作的用途。通过本地仪表盘，集中管理网站、登录信息和执行结果。

| 连接 | 准备 | 使用 |
| --- | --- | --- |
| 添加网站并选择 AI。 | 查看进度和可用操作。 | 执行操作或将其交给 AI 助手。 |

<p align="center">
  <img src="../../docs/images/dashboard.png" alt="Hycli 仪表盘，显示网站卡片、可用操作和任务进度。" width="100%" />
</p>
<p align="center"><sub>包含示例工具和账号的工作区示例。</sub></p>

<a id="get-started"></a>
输入域名，并可在旁边选填使用目的。AI 会先了解网站并选择有用的完整流程，再检查前置步骤、输入和最终结果。如果仍有步骤缺失，网站会保持待检查状态。

## 开始使用

**[下载 v0.1.0 · Linux x64](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)** · glibc ≥ 2.39 · [SHA-256](https://github.com/Hybirdss/Hycli/releases/download/v0.1.0/hycli-0.1.0-linux-x64.tar.gz.sha256)

Hycli 是一个带有本地网页仪表盘的 Rust 应用。在当前检出的源码中构建仪表盘和可执行文件：

```sh
node scripts/build.mjs
./dist/hycli dashboard
```

仪表盘会在 `http://127.0.0.1:4318` 打开。使用 `hycli dashboard --no-open` 可仅输出地址而不打开浏览器，使用 `--port 4320` 可指定其他端口。网站引擎与仪表盘包含在同一个可执行文件中。

1. 打开 **AI 连接**并连接服务提供商。
2. 添加网站地址，选择负责准备该网站的 AI。
3. 查看可用操作。在仪表盘中执行操作，或打开**编程代理**，一次性复制 MCP 配置。

对于需要登录的网站，请从**账号**连接账号。同一网站可以保存多个账号，由你选择工具使用哪一个。

<a id="what-your-ai-can-do"></a>
## AI 能做什么

Hycli 将支持的网站操作转换为具名、带类型的工具。AI 编写的说明会介绍每项操作的用途、所需信息和返回结果。

三个独立的 AI 工作单元分别负责读取和搜索、实用工作流程以及账号验证资料。仪表盘会显示已完成的阶段、每个工作单元的状态、已用时间和通俗易懂的工作记录。CLI 和 MCP 中的操作也会出现在同一活动历史中。

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="三位工作者。一只忙得团团转的小鸟。" width="100%" />
</p>
<p align="center"><sub>三位工作者。一只忙得团团转的小鸟。</sub></p>

准备过程依据网站实际提供的信息：链接的文档、API 模式、公开的 JavaScript，以及浏览器辅助扩展观察到的请求结构。完成规划的工作流后，工作者会继续处理每个尚无操作的已文档化 API 操作，直到每一项都已实现或附上原因报告；任务的 `coverage` 会列出剩余内容。它可以在限定范围内执行读取，以验证操作；不会创建测试记录、编辑内容、删除对象、猜测大量端点，也不会对在线服务器进行模糊测试。

| 操作 | 行为 |
| --- | --- |
| 读取或搜索 | 操作有证据支持且被归类为读取时，可自主执行。 |
| 创建、发送、编辑或删除 | 在你为该网站允许更改之前保持关闭。之后会显示网站、账号、操作和已确定的输入，供你批准。 |
| 效果不明确 | 发送请求前需要审核。 |
| 身份验证、验证挑战或请求限制 | 暂停受影响的请求，让你重新连接或等待。 |

每个网站默认只读。在你于该网站详情中开启**允许更改**之前，其更改类操作不会向 MCP 开放，CLI 也会拒绝；再次关闭会取消待处理的批准。该开关和批准按钮只存在于仪表盘中，且仪表盘只向 `hycli dashboard` 打开的地址授予会话，因此调用 Hycli 的智能体无法开启更改，也无法批准自己的请求。能以你的用户身份运行任意命令的智能体同样可以读取你的文件；请让这类智能体保持在它们各自的权限提示之下。

批准仅适用于一条完全确定的请求，五分钟后过期，且只能使用一次。更改输入、账号、保存的登录信息或已安装的工具定义会使批准失效。CLI、MCP 和仪表盘执行遵守同一边界。写入操作绝不会自动重试。

支持程度取决于网站。如果页面没有可用文档或已观察到的操作，可能需要已登录的浏览器、提供的 SiteSpec 或进一步准备。Hycli 会说明实际支持范围，而不会声称每个网站都有现成的 API。

### 支持范围

支持 JSON 或 YAML OpenAPI、REST、GraphQL，以及 JSON 和 URL 编码表单请求。HTML 操作可通过观察到的选择器提取记录、链接及下一页信息；必要时可用本地 Chromium 读取渲染后的 GET 页面。准备期间若读取失败，AI 会根据证据修正规则并再次验证。

目前不支持任意浏览器点击流程、multipart 上传或二进制下载。分发包不包含账户或已有的生成 CLI。

[操作规范](../SITESPEC.md) · [分发验证](../RELEASING.md)

<a id="ai-connections"></a>
## AI 连接

| 连接 | 登录方式 |
| --- | --- |
| ChatGPT · API 密钥 | 你的 OpenAI API 密钥 |
| Claude | 你的 Anthropic API 密钥 |
| xAI | 你的 xAI API 密钥 |
| Z.ai | 你的 Z.ai API 密钥 |
| Z.ai Coding Plan | 你的 Z.ai Coding Plan API 密钥 |
| ChatGPT · 登录 | 由已安装的 Codex app-server 管理的 ChatGPT 登录 |

在仪表盘中选择模型，也可以选择仅向你的账号开放的模型。连接检查会验证服务提供商和所选模型。API 服务提供商会向你的服务账号收取使用费用。

Codex 连接使用其官方 app-server 协议。Hycli 不读取或复制 Codex OAuth 令牌。推理在临时线程中运行，文件、Shell、浏览器、应用和 MCP 执行均被禁用。网站准备由 Hycli 控制的读取工具完成。

<a id="use-hycli-with-your-ai"></a>
## 将 Hycli 与 AI 配合使用

**编程代理 → 查看连接设置**

通用 MCP 连接涵盖网站准备和执行。连接一次后，智能体即可准备缺少的工具、跟踪进度并运行新操作。

```sh
hycli mcp
```

如需仅公开已安装的操作，请使用 `--sites-only`。以下连接仅限一个网站。

```sh
hycli mcp --sites-only --only-site SITE
```

```json
{
  "mcpServers": {
    "hycli": {
      "command": "hycli",
      "args": [
        "mcp"
      ]
    }
  }
}
```

如果智能体的 PATH 中没有 `hycli`，请使用仪表板显示的可执行文件路径。

CLI 也支持相同流程。请将示例网址、`SITE` 和 `ACTION` 替换为自己的网站及 `describe` 返回的名称。

```sh
hycli prepare https://your-website.example --intent "查找已保存的参考资料"
hycli request SITE "Add draft editing and publishing with full post content"
hycli describe
hycli describe SITE ACTION
hycli SITE ACTION --help
hycli run SITE ACTION --arg query="design systems"
hycli jobs show JOB_ID --watch
```

重试失败的操作之前，先不借助 AI 检查网站：

```sh
hycli check          # 所有已安装的网站
hycli check SITE
```

准备过程会按顺序记录通过的读取，包括把前一个结果的标识符传给下一个读取的情况。`hycli check` 会重放这些读取，并先确认已保存账号的身份。结论会区分 `signed_out`（重新连接账号）与 `site_changed`（账号可用，但响应已不再匹配；请求修复），另外还有 `blocked` 和 `unreachable`。它从不执行更改，退出状态与结论一致。MCP 客户端可通过 `hycli_check` 获得同样的检查。

通过 MCP，向 `hycli_prepare` 提供网址和 `intent`，再用 `hycli_job` 或 `hycli_result` 跟踪至完成。即使客户端尚未刷新工具列表，`hycli_run` 也能运行新操作。需要内部 ID 时，先通过相关列表或搜索操作查找。 `hycli_result`、`hycli_check` 和 `hycli_activity` 在两种模式下都可用，AI 因此可以跟进工作记录、进度和已批准的结果，并区分登录已过期与网站已变化。

修改操作会返回供用户在仪表板审核的批准凭据。请跟踪该凭据的结果，不要重复发送请求。读取失败或响应不符合预期时，CLI 也会返回失败状态。

[智能体指南](../../agent/AGENTS.md) · [Hycli 技能](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## 浏览器登录

准备网站时，首先检查当前操作系统、正在运行或已注册的浏览器，以及可用的浏览器配置文件。AI 根据这些能力读取文档、呈现页面、导入网站会话并验证连接。浏览器和配置文件路径只保留在本地计算机上；生成的网站定义不依赖开发者的安装路径。

在可以访问时，可导入 Chrome、Chromium、Edge、Brave 和 Firefox 的 Cookie 与源站存储数据。只有所请求网站的会话会进入 Hycli 的本地凭据保险库。Hycli 自动选择一个已验证的身份；属于同一身份的配置文件在账号选择时只算一个选项。现有账号选择会保留；如有不同的已验证身份，则需要选择。

在**账号 → 连接账号**中选择**查找我的登录状态**可重新搜索。如果找不到可用会话，选择**打开网站登录页面**，即可在默认浏览器中打开网站。对话框打开期间，Hycli 会再次检查；只有所选账号通过验证后才会继续准备。仅打开标签页不会被报告为登录成功。

受操作系统保护、绑定浏览器、无痕或容器中的会话可能需要浏览器扩展。选择扩展选项卡并生成连接码，然后在已登录的浏览器配置文件中打开扩展、输入连接码并授权访问该网站。原生导入不会绕过 Windows App-Bound 保护或削弱浏览器配置文件的安全性。

- **Chrome 和 Edge：**解压软件包，在浏览器扩展页面选择**加载已解压的扩展程序**。
- **Firefox：**使用 Firefox 软件包，在 `about:debugging` 中选择**临时载入附加组件**。Firefox 重启后会移除临时扩展。此版本不包含经过签名的商店分发版本。
- **Cookie 文件：**也可导入 JSON 或 Netscape 格式的 Cookie 文件作为备用方案。请直接在仪表盘中导入文件，不要将内容粘贴到 AI 对话中。

浏览器会将会话数据直接传输到本地凭据保险库。模型只会收到账号标签、本地密钥的名称和结构，以及已移除凭据值的工具结果。模型可以创建引用这些本地值的连接方案，但不会收到值本身。系统遵守 Cookie 的作用域、路径、过期时间及支持的分区信息；身份验证请求头仅限用于其原始源站。

账号身份以网站实际返回的当前账号响应为依据。浏览器配置文件名称不会被视为已验证的网站身份。如果尚未识别账号，Hycli 会明确说明。连接后，正常浏览即可提供账号响应和可用请求的结构，而不会向准备模型暴露请求值、请求头或响应正文。

网站可能需要另行提供文档中说明的 API 凭据，或需要本地不可用的浏览器功能。准备过程记录实际的连接和读取结果。成功获取首页，或 API 返回登录页面，都不代表集成已经就绪。[浏览器设置指南](../../browser-companion/guide.html)介绍了扩展程序的流程和限制。

<a id="languages"></a>
## 语言

仪表盘支持上方链接的 20 种语言，包括从右向左书写的阿拉伯语。初始语言为英语。你的选择会保存在本设备上，AI 说明也可以使用所选语言生成。

<a id="local-data"></a>
## 本地数据与请求限制

网站定义、账号元数据和活动记录均保存在本地数据目录中。设置 `HYCLI_DATA_DIR` 可选择其他位置。通过**设置**查看当前路径和凭据保护情况。

如果可用，凭据保险库会使用由操作系统密钥环保护的加密密钥。如果密钥环不可用，会明确报告当前存储仅依靠文件权限保护；私有目录和凭据文件仅允许当前操作系统用户访问。已有加密保险库无法解锁时，不会被悄悄替换。

请求按网站和账号串行处理，并控制发送频率，同时还设有网站整体的请求额度。Hycli 遵守 `Retry-After`，限制响应大小和读取重试次数，并在身份验证失败或遇到网站验证挑战时暂停。网站准备过程不会通过轮换账号、伪造指纹或绕过验证挑战来规避限制。这些措施可减少不必要的压力，但无法保证网站永远不会限制账号。

默认禁用私有网络和本地网站地址。对于可信的自托管网站，请在启动仪表盘或 MCP 服务器时明确指定 `--allow-local`。

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

测试使用合成的本地网站和服务提供商响应，覆盖批准与请求的绑定及防重放、秘密信息脱敏、重定向、速率限制、不重试写入、浏览器会话保护和服务提供商响应契约。绝不要仅为测试 Hycli 而使用真实账号创建、修改或删除内容。

截图和 GIF 使用隔离的示例数据。请参阅[视觉素材来源](../images/README.md)。

## 许可证与预期用途

Hycli 采用 [Apache-2.0](../../LICENSE) 许可证。

Hycli 用于创建和使用让网站更易操作的命令行工具。请仅用于你有权访问的网站和账号。

本软件按原样提供，不附带任何保证。在法律允许的范围内，作者和贡献者不对使用本软件所造成的损失或其他问题承担责任。你应对自己的使用方式负责。

在法律允许的范围内，作者和贡献者不对因使用 Hycli 而导致的账号封禁、暂停或限制承担责任。请勿将 Hycli 用于入侵、未经授权的访问或攻击。
