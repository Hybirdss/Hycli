<p align="center">
  <img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="將網站變成 AI 可用的工具。" width="100%" />
</p>

<p align="center">
  <strong>將任何網站轉化為可供 AI 代理與終端機直接使用的具型別 CLI 與 MCP 工具。</strong><br/>
  <sub>直接沿用真實瀏覽器登入狀態，無須擔心向大型語言模型外洩 Cookie 或遭反爬蟲封鎖，輕鬆實現讀取、搜尋、建立、編輯與自動化。</sub>
</p>

<p align="center">
  <a href="https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0">下載 (Windows · macOS · Linux)</a> ·
  <a href="https://github.com/Hybirdss/Hycli/issues">在 GitHub 上取得協助</a> ·
  <a href="#try-it-in-30-seconds">30 秒快速體驗</a> ·
  <a href="#get-started">開始使用</a> ·
  <a href="#what-your-ai-can-do">AI 能做什麼</a> ·
  <a href="#common-concerns">常見疑慮與解決方式</a> ·
  <a href="#use-hycli-with-your-ai">搭配 AI 使用</a>
</p>

<details>
<summary>選擇閱讀語言 · 20 種語言</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

只需輸入任何網站網址與你的目的。三個 AI 工作單元會分析文件、前端 JS 套件與真實瀏覽器網路流量，建構涵蓋讀取與寫入操作的具型別 CLI 與 MCP 工具集——且在準備過程中絕不會向你的真實帳號發送任何寫入請求。

<p align="center">
  <img src="../../docs/images/dashboard.png" alt="Hycli 儀表板，顯示網站卡片、可用操作與工作進度。" width="100%" />
</p>
<p align="center"><sub>在同一個本機儀表板與 CLI 中集中管理網站、帳號、寫入權限與執行結果。</sub></p>

<a id="try-it-in-30-seconds"></a>
## 30 秒快速體驗

```sh
# 1. 指定任何網站並描述你的目標（同時發掘讀取與寫入操作，準備期間零真實寫入）
hycli prepare https://your-website.example --intent "搜尋文章、儲存草稿並發佈新文章"

# 2. 一覽所有已準備好的讀取、寫入指令與推薦的多步驟工作流程
hycli describe your-website

# 3. 立即執行讀取或搜尋操作
hycli your-website search-posts --query "版本發佈說明"

# 4. 需要建立、編輯或刪除時，為該 CLI 一次性開啟寫入權限即可直接執行
hycli writes your-website allow
hycli your-website create-draft --title "來自 Hycli 的問候" --body "直接從終端機自動建立。"
```

或者在 **Claude Code、Cursor、Codex、Windsurf** 中一次性加入 `"command": "hycli", "args": ["mcp"]`，接著用自然語言請代理執行：
> *「用 Hycli 準備 `https://...` 的 CLI，整理有哪些可用操作，然後幫我建立一篇新草稿。」*

<a id="get-started"></a>
## 開始使用

Hycli 將網站引擎與本機儀表板（`http://127.0.0.1:4318`）內建於單一原生執行檔中。執行發佈套件無須安裝 Rust 或 Node.js。

### 下載與執行 (Windows, macOS, Linux)

前往 **[GitHub Releases (v0.1.0)](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)** 下載適合你作業系統的原生安裝檔或可攜式壓縮檔：

| 作業系統 | 架構 | 原生安裝檔 | 可攜式壓縮檔 | 啟動方式 |
| --- | --- | --- | --- | --- |
| **Windows 10/11** | x64 及 ARM64 | `hycli-0.1.0-win32-*-setup.exe` | `hycli-0.1.0-win32-*.tar.gz` | 執行 `*-setup.exe`（免系統管理員權限，自動加入使用者 `PATH` 與開始功能表），接著點選 **Hycli** 圖示或在 PowerShell 執行 `hycli open` |
| **macOS 13+** | Apple Silicon (`arm64`) 及 Intel (`x64`) | `hycli-0.1.0-darwin-*.dmg` | `hycli-0.1.0-darwin-*.tar.gz` | 開啟 `.dmg`，將 **Hycli.app** 拖曳至 **Applications** 後開啟（自動連結 `~/.local/bin/hycli`） |
| **Linux** | x64 及 ARM64 | `hycli-0.1.0-linux-*.deb` | `hycli-0.1.0-linux-*.tar.gz` | 安裝 `.deb`（`sudo apt install ./hycli-*.deb`）或解壓縮 `.tar.gz` 後執行 `./install.sh`，接著執行 `hycli open` |

<details>
<summary><strong>終端機可攜式安裝與從原始碼建置</strong></summary>

**Linux / macOS 可攜式套件安裝：**

```sh
sha256sum -c hycli-0.1.0-*.tar.gz.sha256   # macOS 請使用 shasum -a 256 -c
tar -xzf hycli-0.1.0-*.tar.gz
cd hycli-0.1.0-*
./install.sh
hycli open
```

**Windows PowerShell 可攜式套件安裝：**

```powershell
tar -xzf hycli-0.1.0-win32-x64.tar.gz
cd hycli-0.1.0-win32-x64
powershell -ExecutionPolicy Bypass -File .\install.ps1
hycli open
```

**從原始碼建置 (Linux, macOS, Windows)：**

安裝[建置前置需求](../BUILD.md)後執行以下指令：

```sh
node scripts/build.mjs
./dist/hycli open
```

若要產生目前作業系統專用的原生安裝檔（`.exe`、`.dmg`、`.deb`），請執行 `node scripts/package-desktop.mjs`。詳情請參閱[建置與封裝說明](../BUILD.md)及[發佈驗證](../RELEASING.md)。

</details>

### 連接第一個網站

1. 執行 `hycli open`（或點選 **Hycli** 應用程式圖示）開啟本機儀表板（`http://127.0.0.1:4318`）。
2. 在 **AI 連線**中連接 OpenAI、Anthropic Claude、xAI、Z.ai 或本機 ChatGPT (Codex) 登入。
3. 針對需要登入的網站，點選 **帳號 → 尋找我的登入狀態** 將瀏覽器工作階段匯入本機加密保管庫。
4. 輸入網站網址與目的以產生工具，或從 **程式代理** 複製 MCP 設定並貼到你的 AI 助理中。

<a id="what-your-ai-can-do"></a>
## AI 能做什麼

Hycli 將網站的各種真實介面轉換為具自我說明能力的 CLI 子指令與 MCP 工具：

| 網站介面 | 準備好的功能 |
| --- | --- |
| REST 與 OpenAPI | 基於 JSON/YAML 結構描述與 API 文件的具型別讀取與寫入請求 |
| GraphQL | 解析後的查詢與變更（mutation）文件及具型別的 JSON 變數 |
| HTML 與 JS 渲染頁面 | 基於所觀察選擇器的清單、連結、分頁擷取，以及透過本機 Chromium 引擎讀取渲染後的 GET 頁面 |
| JSON 與 URL 編碼表單 | 具文件說明的輸入值、預設值，以及綁定帳號的表單請求（`POST`、`PUT`、`PATCH`、`DELETE`） |

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="三位工作者。一隻忙得團團轉的小鳥。" width="100%" />
</p>
<p align="center"><sub>三位工作者。一隻忙得團團轉的小鳥。</sub></p>

| 操作類型 | 執行行為 |
| --- | --- |
| **讀取或搜尋** | 只要有證據支持且歸類為安全讀取，即可立即執行。 |
| **建立、傳送、編輯或刪除** | 在準備過程中會一併發掘並合成至 CLI 規格中，但在準備與檢查期間絕不會執行。每個網站 CLI 預設關閉寫入（`writes: false`）。AI 代理會**依每個 CLI 僅詢問你一次**是否允許寫入（`hycli writes SITE allow` 或 MCP `hycli_writes`），開啟後即可依你的指示直接執行寫入操作。 |
| **影響不明確的操作** | 在傳送請求前暫緩並交由審查。 |
| **身分驗證挑戰或速率限制** | 立即切換傳輸策略（HTTP → 頁面內 `window.fetch` → 隱身 Chromium）或安全暫停。 |

支援的請求形式與擷取規則請參閱[操作規格](../SITESPEC.md)。

<a id="common-concerns"></a>
## 常見疑慮與 Hycli 的解決方式

將 AI 代理連接到真實網站與已登入帳號時，使用者最常擔心的問題以及 Hycli 的技術解決方案：

### 1. 「AI 會不會在我的帳號裡隨意發文、修改或誤刪資料？」
- **建立與檢查 CLI 期間保證零真實寫入：** 執行 `hycli prepare`、`hycli request`、`hycli recon`、`hycli check` 準備或檢查網站時，Hycli 會發掘並編譯讀取與寫入操作，但**絕不會向伺服器傳送任何真實寫入請求**。即使是偽裝成讀取的變更路徑（如包含 `/delete` 的 `GET`、GraphQL `mutation`、HTML `<form method="post">` 送出），在驗證階段也會被嚴格攔截。
- **依 CLI 首次寫入前的一次性使用者確認：** 每個新產生的網站 CLI 預設皆為關閉寫入狀態（`writes: false`）。當代理首次需要在某個 CLI 上執行發文、修改或刪除等寫入功能時，會**針對該 CLI 詢問你一次**：*「是否允許在 `<site>` 執行寫入操作？」* 只有你同意並開啟 `hycli writes <site> allow`（或 MCP `hycli_writes`）後，寫入操作才會直接執行；你隨時可以透過 `hycli writes <site> deny` 或儀表板的 **允許變更** 開關收回權限。
- **失敗的寫入絕不自動重試：** 失敗或中斷的寫入請求絕不會自動重試。若傳送變更後連線中斷，Hycli 會回傳 `action_outcome_unknown` 讓你先確認網站狀態，避免重複下單或重複發文。

### 2. 「Hycli 會把我的登入 Cookie、權杖或 API 金鑰傳送給 AI 模型（LLM）嗎？」
- **機密值零進入 AI 提示詞：** 瀏覽器 Cookie、`localStorage` / `sessionStorage` 權杖與 API 金鑰僅儲存在本機加密保管庫中（透過 Windows 認證管理員、macOS 鑰匙圈、Linux Secret Service 加密，或限制為 `0600` 使用者專屬權限檔案）。
- **符號化連線配方（`ConnectionRecipe`）：** AI 模型只會收到帳號標籤、儲存空間鍵名與遮蔽後的回應結構。模型僅建立參照本機鍵名的符號化 `ConnectionRecipe`，實際機密值由 Hycli 在執行時於本機注入，並自動從所有輸出與記錄檔中遮蔽。
- **依網站隔離的唯讀工作階段匯入：** 匯入瀏覽器工作階段時僅透過臨時唯讀快照讀取目標網域的 Cookie 與儲存空間，絕不削弱瀏覽器安全性或匯出其他網站的 Cookie。

### 3. 「擷取資料時會不會觸發驗證碼（CAPTCHA）或導致帳號被封鎖？」
- **禁止模糊測試（Fuzzing）與暴力掃描：** Hycli 絕不猜測端點清單或對運作中的伺服器進行模糊測試，僅從連結文件、OpenAPI/GraphQL 結構描述、靜態 JS 套件與觀察到的真實瀏覽器網路結構學習 API。
- **依網站與帳號限流循序傳送：** 請求依網站與帳號循序排隊並控制速率，嚴格遵守 `Retry-After` 標頭，並限制讀取重試次數與回應大小。
- **策略斷路器與真實瀏覽器環境執行：** 當一般 HTTP 請求遇到 403/429 或反爬蟲挑戰（如 Cloudflare、Akamai Bot Manager）時，Hycli 絕不會無限重試被封鎖的請求。斷路器會立即切換傳輸方式——在已驗證的真實瀏覽器環境內執行 `window.fetch(..., { credentials: 'include' })` 或切換至本機隱身 Chromium 引擎——或安全暫停以保護帳號。

### 4. 「重新導向會不會外洩工作階段，或分享規格檔會不會暴露我電腦的路徑？」
- **嚴格的來源綁定（Origin Binding）：** 驗證標頭與 Cookie 嚴格鎖定於原始來源（Origin），跨網域重新導向時會立即移除。私有網路與本機位址預設封鎖，除非明確指定 `--allow-local`。
- **可攜式規格檔：** 本機瀏覽器執行檔路徑與設定檔路徑僅保留在裝置內部狀態中，絕不會寫入可攜式的 `SiteSpec` 檔案。

### 5. 「當登入過期時，AI 會不會浪費 Token 一直重試？」
- **無須 AI 的確定性狀態診斷（`hycli check`）：** `hycli check`（及 MCP `hycli_check`）無須呼叫大型語言模型即可重播已驗證的讀取鏈，精確區分 `signed_out`（登入過期，需重新連接帳號）、`site_changed`（網站結構變更，需修復規格）、`blocked` 與 `unreachable`。

<a id="ai-connections"></a>
## AI 連線

| 服務供應商 | 驗證方式 |
| --- | --- |
| ChatGPT · API 金鑰 | OpenAI API 金鑰 |
| Claude | Anthropic API 金鑰 |
| xAI | xAI API 金鑰 |
| Z.ai / Z.ai Coding Plan | Z.ai API 金鑰 |
| ChatGPT · 登入 | 透過官方 Codex app-server 協定管理的本機 ChatGPT 登入 |

Codex 登入連線在停用檔案、Shell、瀏覽器與外部 MCP 執行的臨時執行緒中運作；Hycli 絕不讀取或複製 Codex OAuth 權杖。

<a id="use-hycli-with-your-ai"></a>
## 搭配 AI 使用 Hycli

將 Hycli 加入你的 MCP 用戶端（**程式代理 → 查看連線設定** 會顯示目前作業系統的正確路徑）：

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

若只要公開已安裝的網站工具，請使用 `hycli mcp --sites-only`；若要限制為單一網站，請加上 `--only-site SITE`。MCP 伺服器提供 `hycli_prepare`、`hycli_writes`（依每個 CLI 首次詢問使用者後開啟寫入）、`hycli_run`、`hycli_check`、`hycli_result` 與 `hycli_activity`，且絕不提供任何讀取憑證的工具。

在終端機中也能使用完全相同的工作流程：

```sh
hycli prepare https://your-website.example --intent "尋找已儲存的參考資料"
hycli request SITE "新增文章內文編輯、草稿儲存與發佈功能"
hycli describe SITE
hycli SITE ACTION --help
hycli writes SITE allow
hycli SITE ACTION --query "design systems"
hycli check SITE
```

[代理指南](../../agent/AGENTS.md) · [Hycli 技能](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## 瀏覽器登入

Hycli 會在 Windows、macOS 與 Linux 上探索本機 Chrome、Chromium、Edge、Brave、Vivaldi、Arc、Whale 與 Firefox 設定檔（以及已安裝的本機隱身 Chromium 引擎），僅將所請求網站的工作階段匯入本機加密保管庫。共用同一個已驗證網站帳號的多個設定檔會自動合併去重。

若工作階段受 Windows App-Bound 加密、私密容器或未匯入的瀏覽器保護，可在 **帳號 → 連接帳號** 中透過瀏覽器登入、連接[瀏覽器擴充功能](../../browser-companion/guide.html)，或在儀表板中匯入 JSON/Netscape Cookie 檔案（請勿將 Cookie 內容直接貼到 AI 聊天視窗中）。

<a id="languages"></a>
## 語言

儀表板與 AI 產生的操作說明支援本 README 頂端連結的 20 種語言，包含由右至左書寫的阿拉伯文（RTL）。

<a id="local-data"></a>
## 本機資料與請求限制

網站定義、加密憑證與活動紀錄保存在作業系統的本機資料目錄中（Windows 為 `%LOCALAPPDATA%\hycli`，macOS 為 `~/Library/Application Support/hycli`，Linux 為 `~/.local/share/hycli`，或透過 `HYCLI_DATA_DIR` 指定）。你隨時可以在 **設定** 中查看目前路徑與金鑰環加密狀態。

<a id="contributing"></a>
## 開發與驗證

```sh
npm --prefix dashboard ci
npm --prefix dashboard run check
npm --prefix dashboard run build
cargo test --all-targets --locked
cargo build --locked --bin hycli --example dashboard_fixture
node scripts/test-e2e.mjs --full
```

所有測試皆使用合成的本機網站與測試夾具執行——請勿為了測試而在真實的第三方服務帳號上執行寫入操作。另請參閱[視覺素材來源](../images/README.md)。

## 授權與預期用途

Hycli 採用 [Apache-2.0](../../LICENSE) 授權條款發佈。請僅在你有權存取的網站與帳號上使用。本軟體依「現狀」提供，不附帶任何明示或默示之擔保；作者與貢獻者不對因使用本軟體而造成的任何損失、帳號限制或封鎖負責。

## Star History

<a href="https://www.star-history.com/#Hybirdss/Hycli&Date">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date&theme=dark" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
 </picture>
</a>
