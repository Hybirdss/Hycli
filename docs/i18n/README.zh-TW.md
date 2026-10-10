<p align="center">
  <img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="將網站變成 AI 可用的工具。" width="100%" />
</p>

<p align="center">
  <strong>自動化任何網站。</strong>
</p>

<p align="center">
  <a href="#get-started">開始使用</a> ·
  <a href="#what-your-ai-can-do">AI 能做什麼</a> ·
  <a href="#ai-connections">AI 連線</a> ·
  <a href="#use-hycli-with-your-ai">搭配 AI 使用</a>
</p>

<details>
<summary>選擇閱讀語言 · 20 種語言</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

新增網站後，Hycli 會準備好 AI 可用的操作，並說明每項操作的用途。在本機儀表板中集中管理網站、登入資訊與執行結果。

| 連線 | 準備 | 使用 |
| --- | --- | --- |
| 新增網站並選擇 AI。 | 查看進度與可用操作。 | 執行操作，或交給 AI 助理使用。 |

<p align="center">
  <img src="../../docs/images/dashboard.png" alt="Hycli 儀表板，顯示網站卡片、可用操作與工作進度。" width="100%" />
</p>
<p align="center"><sub>包含範例工具和帳號的工作區範例。</sub></p>

<a id="get-started"></a>
輸入網域，並可在旁邊選填使用目的。AI 會先了解網站並選擇實用的完整流程，再檢查前置步驟、輸入和最終結果。如果仍有步驟缺漏，網站會保持待檢查狀態。

## 開始使用

**[下載 v0.1.0 · Linux x64](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)** · glibc ≥ 2.39 · [SHA-256](https://github.com/Hybirdss/Hycli/releases/download/v0.1.0/hycli-0.1.0-linux-x64.tar.gz.sha256)

Hycli 是附有本機網頁儀表板的 Rust 應用程式。請在目前取出的原始碼中建置儀表板與執行檔：

```sh
node scripts/build.mjs
./dist/hycli dashboard
```

儀表板會在 `http://127.0.0.1:4318` 開啟。使用 `hycli dashboard --no-open` 可只輸出網址而不開啟瀏覽器，使用 `--port 4320` 可指定其他連接埠。網站引擎與儀表板包含在同一個執行檔中。

1. 開啟 **AI 連線**並連接服務供應商。
2. 新增網站網址，選擇負責準備該網站的 AI。
3. 檢視可用操作。在儀表板執行操作，或開啟**程式代理**，一次複製 MCP 設定。

需要登入的網站，請從**帳號**連接帳號。同一網站可儲存多個帳號，由你選擇工具要使用哪一個。

<a id="what-your-ai-can-do"></a>
## AI 能做什麼

Hycli 將支援的網站操作轉換為具名且具型別的工具。AI 撰寫的說明會介紹各項操作的用途、所需資訊及回傳結果。

三個獨立的 AI 工作單元分別負責讀取與搜尋、實用工作流程，以及帳號驗證資料。儀表板會顯示已完成的階段、各工作單元的狀態、經過時間和易懂的工作紀錄。CLI 與 MCP 的工作也會出現在同一份活動記錄中。

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="三位工作者。一隻忙得團團轉的小鳥。" width="100%" />
</p>
<p align="center"><sub>三位工作者。一隻忙得團團轉的小鳥。</sub></p>

準備過程依據網站實際提供的資訊：連結的文件、API 結構描述、公開的 JavaScript，以及瀏覽器輔助擴充功能觀察到的請求結構。完成規劃的工作流程後，工作者會繼續處理每個尚無操作的已文件化 API 操作，直到每一項都已實作或附上原因回報；工作的 `coverage` 會列出剩餘內容。它可以在限定範圍內讀取，以確認操作；不會建立測試記錄、編輯內容、刪除物件、猜測大量端點，也不會對運作中的伺服器進行模糊測試。

| 操作 | 行為 |
| --- | --- |
| 讀取或搜尋 | 有證據支持且歸類為讀取操作時，可自主執行。 |
| 建立、傳送、編輯或刪除 | 在你為該網站允許變更之前保持關閉。之後會顯示網站、帳號、操作與已確定的輸入，供你核准。 |
| 影響不明 | 傳送請求前需要審查。 |
| 身分驗證、驗證挑戰或請求限制 | 暫停受影響的請求，讓你重新連線或等待。 |

每個網站預設為唯讀。在你於該網站詳細資料中開啟**允許變更**之前，其變更類操作不會開放給 MCP，CLI 也會拒絕；再次關閉會取消待處理的核准。該開關與核准按鈕只存在於儀表板中，且儀表板只對 `hycli dashboard` 開啟的位址授予工作階段，因此呼叫 Hycli 的代理程式無法開啟變更，也無法核准自己的請求。能以你的使用者身分執行任意指令的代理程式同樣可以讀取你的檔案；請讓這類代理程式保持在它們各自的權限提示之下。

核准只適用於一筆完全確定的請求，五分鐘後失效，且只能使用一次。變更輸入、帳號、儲存的登入資訊或已安裝的工具定義會使核准失效。CLI、MCP 與儀表板執行遵循相同界線。寫入操作絕不會自動重試。

支援程度取決於網站。沒有可用文件或已觀察操作的頁面，可能需要已登入的瀏覽器、提供的 SiteSpec 或進一步準備。Hycli 會說明實際能支援的範圍，而不會宣稱每個網站都有現成的 API。

### 支援範圍

支援 JSON 或 YAML OpenAPI、REST、GraphQL，以及 JSON 和 URL 編碼表單請求。HTML 操作可透過觀察到的選擇器擷取記錄、連結及下一頁資訊；必要時可使用本機 Chromium 讀取渲染後的 GET 頁面。準備期間若讀取失敗，AI 會依據證據修正定義並再次驗證。

目前不支援任意瀏覽器點擊流程、multipart 上傳或二進位下載。發佈套件不包含帳戶或既有的生成 CLI。

[操作規格](../SITESPEC.md) · [發佈驗證](../RELEASING.md)

<a id="ai-connections"></a>
## AI 連線

| 連線 | 登入方式 |
| --- | --- |
| ChatGPT · API 金鑰 | 你的 OpenAI API 金鑰 |
| Claude | 你的 Anthropic API 金鑰 |
| xAI | 你的 xAI API 金鑰 |
| Z.ai | 你的 Z.ai API 金鑰 |
| Z.ai Coding Plan | 你的 Z.ai Coding Plan API 金鑰 |
| ChatGPT · 登入 | 由已安裝的 Codex app-server 管理的 ChatGPT 登入 |

在儀表板選擇模型，也可選擇僅向你的帳號開放的模型。連線檢查會驗證供應商與所選模型。API 供應商會向你的服務帳號收取使用費用。

Codex 連線使用官方 app-server 協定。Hycli 不會讀取或複製 Codex OAuth 權杖。推論在臨時執行緒中執行，檔案、Shell、瀏覽器、應用程式與 MCP 執行均已停用。網站準備由 Hycli 控制的讀取工具完成。

<a id="use-hycli-with-your-ai"></a>
## 搭配 AI 使用 Hycli

**程式代理 → 查看連線設定**

一般 MCP 連線涵蓋網站準備與執行。連線一次後，代理即可準備缺少的工具、追蹤進度並執行新操作。

```sh
hycli mcp
```

若只要公開已安裝的操作，請使用 `--sites-only`。以下連線僅限一個網站。

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

如果代理的 PATH 中沒有 `hycli`，請使用儀表板顯示的執行檔路徑。

CLI 也支援相同流程。請將範例網址、`SITE` 和 `ACTION` 替換為自己的網站及 `describe` 傳回的名稱。

```sh
hycli prepare https://your-website.example --intent "尋找已儲存的參考資料"
hycli request SITE "Add draft editing and publishing with full post content"
hycli describe
hycli describe SITE ACTION
hycli SITE ACTION --help
hycli run SITE ACTION --arg query="design systems"
hycli jobs show JOB_ID --watch
```

重試失敗的操作之前，先不借助 AI 檢查網站：

```sh
hycli check          # 所有已安裝的網站
hycli check SITE
```

準備過程會依序記錄通過的讀取，包括把前一個結果的識別碼傳給下一個讀取的情況。`hycli check` 會重新執行這些讀取，並先確認已儲存帳號的身分。判定會區分 `signed_out`（重新連線帳號）與 `site_changed`（帳號可用，但回應已不再相符；請求修復），另外還有 `blocked` 與 `unreachable`。它從不執行變更，結束狀態與判定一致。MCP 用戶端可透過 `hycli_check` 取得同樣的檢查。

透過 MCP，將網址和 `intent` 傳給 `hycli_prepare`，再用 `hycli_job` 或 `hycli_result` 追蹤至完成。即使客戶端尚未更新工具清單，`hycli_run` 也能執行新操作。需要內部 ID 時，先透過相關清單或搜尋操作查找。 `hycli_result`、`hycli_check` 和 `hycli_activity` 在兩種模式下都可使用，AI 因此可以追蹤工作記錄、進度與已核准的結果，並區分登入已過期與網站已變更。

修改操作會傳回供使用者在儀表板審核的核准憑據。請追蹤該憑據的結果，不要重複傳送請求。讀取失敗或回應不符預期時，CLI 也會傳回失敗狀態。

[代理指南](../../agent/AGENTS.md) · [Hycli 技能](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## 瀏覽器登入

準備網站時，首先檢查目前的作業系統、執行中或已註冊的瀏覽器，以及可用的瀏覽器設定檔。AI 根據這些功能讀取文件、呈現頁面、匯入網站工作階段並驗證連線。瀏覽器和設定檔路徑只保留在本機電腦上；產生的網站定義不依賴開發者的安裝路徑。

可以存取時，可匯入 Chrome、Chromium、Edge、Brave 和 Firefox 的 Cookie 與來源儲存資料。只有所要求網站的工作階段會進入 Hycli 的本機憑證保管庫。Hycli 會自動選擇單一已驗證身分；屬於同一身分的設定檔在帳號選擇時只算一個選項。既有帳號選擇會保留；若有不同的已驗證身分，則需要選擇。

在**帳號 → 連接帳號**中選擇**尋找我的登入狀態**即可重新搜尋。如果找不到可用的工作階段，選擇**開啟網站登入頁面**，便會在預設瀏覽器中開啟網站。對話方塊開啟期間，Hycli 會重新檢查；只有選取的帳號通過驗證後才會繼續準備。僅開啟分頁不會被視為登入成功。

受作業系統保護、綁定瀏覽器、私密或容器中的工作階段可能需要瀏覽器擴充功能。選擇擴充功能分頁並產生連線代碼，接著在已登入的瀏覽器設定檔中開啟擴充功能、輸入代碼，並授予該網站的存取權限。原生匯入不會繞過 Windows App-Bound 保護或降低瀏覽器設定檔的安全性。

- **Chrome 與 Edge：**解壓縮套件，在瀏覽器擴充功能頁面選擇**載入未封裝項目**。
- **Firefox：**使用 Firefox 套件，在 `about:debugging` 選擇**載入暫用附加元件**。Firefox 重新啟動後會移除暫用擴充功能。本次版本未包含經簽署的商店發佈版本。
- **Cookie 檔案：**也可匯入 JSON 與 Netscape 格式的 Cookie 檔案作為替代方案。請直接在儀表板匯入檔案，不要將內容貼到 AI 對話中。

瀏覽器會將工作階段資料直接傳送到本機憑證保管庫。模型只會收到帳號標籤、本機金鑰的名稱和結構，以及已移除憑證值的工具結果。模型可以建立參照這些本機值的連線方案，但不會收到值本身。系統遵循 Cookie 的作用範圍、路徑、到期時間與支援的分割區資訊；驗證標頭僅限用於其原始來源。

帳號身分依據網站實際回傳的目前帳號回應確認。瀏覽器設定檔名稱不會被視為已驗證的網站身分。如果尚未識別帳號，Hycli 會明確說明。連線後，正常瀏覽就能提供帳號回應與可用請求的結構，同時不會向準備模型揭露請求值、標頭或回應本文。

網站可能需要另外提供文件記載的 API 憑證，或需要本機無法使用的瀏覽器功能。準備過程會記錄實際的連線和讀取結果。成功取得首頁，或 API 回傳登入頁面，都不表示整合已準備就緒。[瀏覽器設定指南](../../browser-companion/guide.html)說明了擴充功能的流程與限制。

<a id="languages"></a>
## 語言

儀表板支援上方連結的 20 種語言，包括由右至左書寫的阿拉伯文。初始語言為英文。你的選擇會儲存在這部裝置上，AI 說明也可使用所選語言產生。

<a id="local-data"></a>
## 本機資料與請求限制

網站定義、帳號中繼資料與活動紀錄會留在本機資料目錄。設定 `HYCLI_DATA_DIR` 可選擇其他位置。透過**設定**查看目前位置與憑證保護狀態。

可用時，憑證保管庫會使用由作業系統金鑰環保護的加密金鑰。若無法使用金鑰環，系統會明確指出目前儲存方式僅以檔案權限保護；私人目錄與憑證檔案僅限目前作業系統使用者存取。既有加密保管庫無法解鎖時，不會被悄悄替換。

請求依網站與帳號循序處理，並控制傳送頻率，另外設有網站整體的請求額度。Hycli 遵循 `Retry-After`，限制回應大小與讀取重試次數，並在驗證失敗或遇到網站驗證挑戰時暫停。網站準備過程不會藉由輪換帳號、偽造指紋或繞過驗證挑戰來規避限制。這些措施可減少不必要的負荷，但無法保證網站永遠不會限制帳號。

預設停用私人網路與本機網站位址。若要使用可信任的自行架設網站，請在啟動儀表板或 MCP 伺服器時明確指定 `--allow-local`。

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

測試使用合成的本機網站與供應商回應，涵蓋核准與請求的綁定及防重放、機密資訊遮蔽、重新導向、速率限制、不重試寫入、瀏覽器工作階段保護與供應商回應契約。絕不要僅為測試 Hycli 而使用真實帳號建立、修改或刪除內容。

螢幕擷取畫面與 GIF 使用隔離的範例資料。請參閱[視覺素材來源](../images/README.md)。

## 授權與預期用途

Hycli 採用 [Apache-2.0](../../LICENSE) 授權。

Hycli 用於建立和使用讓網站更容易操作的命令列工具。請用於你有權存取的網站與帳號。

本軟體依現狀提供，不附帶任何保證。在法律允許的範圍內，作者與貢獻者不對使用本軟體所造成的損失或其他問題負責。你應對自己的使用方式負責。

在法律允許的範圍內，作者及貢獻者不對因使用 Hycli 而導致的帳號封禁、暫停或限制承擔責任。請勿將 Hycli 用於入侵、未經授權的存取或攻擊。
