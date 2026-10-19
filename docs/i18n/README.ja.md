<p align="center">
  <img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="ウェブサイトを、AIが使えるツールに。" width="100%" />
</p>

<p align="center">
  <strong>あらゆるウェブサイトを、AIエージェントとターミナルでそのまま使える型付きCLI・MCPツールに変換。</strong><br/>
  <sub>普段のブラウザーのログイン状態をそのまま活用し、LLMへのCookie漏洩ゼロ・ボット対策への耐性を備えたまま、読み取り・検索から作成・編集・自動化まで行えます。</sub>
</p>

<p align="center">
  <a href="https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0">ダウンロード (Windows · macOS · Linux)</a> ·
  <a href="https://github.com/Hybirdss/Hycli/issues">GitHubでサポートを受ける</a> ·
  <a href="#try-it-in-30-seconds">30秒で試す</a> ·
  <a href="#get-started">はじめに</a> ·
  <a href="#what-your-ai-can-do">AIでできること</a> ·
  <a href="#common-concerns">よくある懸念と対策</a> ·
  <a href="#use-hycli-with-your-ai">AIで使う</a>
</p>

<details>
<summary>お好きな言語で読む · 20言語</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

ウェブサイトのURLとやりたいことを入力するだけ。3つのAIワーカーがドキュメント、フロントエンドのJSバンドル、実際のブラウザー通信を解析し、読み取りと書き込みの両方を網羅した型付きCLIおよびMCPツールセットを構築します。準備中に実際のアカウントへ書き込みリクエストを送信することは一切ありません。

<p align="center">
  <img src="../../docs/images/dashboard.png" alt="ウェブサイトのカード、利用可能な操作、タスクの進捗を表示するHycliダッシュボード。" width="100%" />
</p>
<p align="center"><sub>ウェブサイト、アカウント、書き込み権限、実行結果をローカルのダッシュボードとCLIで一括管理。</sub></p>

<a id="try-it-in-30-seconds"></a>
## 30秒で試す

```sh
# 1. 任意のウェブサイトと目的を指定してCLIを準備（読み取り・書き込みの両方を検出、準備中の実環境への変更は0件）
hycli prepare https://your-website.example --intent "記事の検索、下書きの保存、新しい記事の公開"

# 2. 準備された読み取り・書き込みコマンドと複数ステップのワークフローを一覧表示
hycli describe your-website

# 3. 読み取り・検索アクションはすぐに実行可能
hycli your-website search-posts --query "リリースノート"

# 4. 作成・編集・削除を行う準備ができたら、CLIごとに1回だけ書き込みを許可して実行
hycli writes your-website allow
hycli your-website create-draft --title "Hycliからのテスト" --body "ターミナルから自動化しました。"
```

または、**Claude Code、Cursor、Codex、Windsurf** に `"command": "hycli", "args": ["mcp"]` を一度登録し、自然言語でエージェントに指示するだけです:
> *「Hycliで `https://...` を準備して、使えるアクションを教えて。それから新しい記事の下書きを作成して。」*

<a id="get-started"></a>
## はじめに

Hycliはウェブサイトエンジンとローカルダッシュボード（`http://127.0.0.1:4318`）を単一のネイティブバイナリに内蔵しています。配布パッケージの実行にRustやNode.jsは不要です。

### ダウンロードと実行 (Windows, macOS, Linux)

お使いのプラットフォーム向けのネイティブインストーラーまたはポータブルアーカイブを **[GitHub Releases (v0.1.0)](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)** からダウンロードしてください:

| OS | アーキテクチャ | ネイティブインストーラー | ポータブルアーカイブ | 起動方法 |
| --- | --- | --- | --- | --- |
| **Windows 10/11** | x64 および ARM64 | `hycli-0.1.0-win32-*-setup.exe` | `hycli-0.1.0-win32-*.tar.gz` | `*-setup.exe` を実行（管理者権限不要、ユーザー `PATH` とスタートメニューに自動登録）し、**Hycli** を開くか PowerShell で `hycli open` を実行 |
| **macOS 13+** | Apple Silicon (`arm64`) および Intel (`x64`) | `hycli-0.1.0-darwin-*.dmg` | `hycli-0.1.0-darwin-*.tar.gz` | `.dmg` を開き、**Hycli.app** を **Applications** にドラッグして起動（`~/.local/bin/hycli` に自動リンク） |
| **Linux** | x64 および ARM64 | `hycli-0.1.0-linux-*.deb` | `hycli-0.1.0-linux-*.tar.gz` | `.deb` をインストール（`sudo apt install ./hycli-*.deb`）するか `.tar.gz` を展開して `./install.sh` を実行後、`hycli open` を実行 |

<details>
<summary><strong>ターミナルからのポータブルインストールとソースからのビルド</strong></summary>

**Linux / macOS ポータブルアーカイブ:**

```sh
sha256sum -c hycli-0.1.0-*.tar.gz.sha256   # macOS では shasum -a 256 -c
tar -xzf hycli-0.1.0-*.tar.gz
cd hycli-0.1.0-*
./install.sh
hycli open
```

**Windows PowerShell ポータブルアーカイブ:**

```powershell
tar -xzf hycli-0.1.0-win32-x64.tar.gz
cd hycli-0.1.0-win32-x64
powershell -ExecutionPolicy Bypass -File .\install.ps1
hycli open
```

**ソースからビルド (Linux, macOS, Windows):**

[ビルドの前提条件](../BUILD.md)をインストールして以下を実行します:

```sh
node scripts/build.mjs
./dist/hycli open
```

現在のOS向けのネイティブインストーラー（`.exe`、`.dmg`、`.deb`）を生成するには `node scripts/package-desktop.mjs` を実行します。詳細は[ビルドとパッケージング](../BUILD.md)および[リリース検証](../RELEASING.md)を参照してください。

</details>

### 最初のウェブサイトを接続する

1. `hycli open` を実行する（または **Hycli** アプリアイコンをクリックする）と、ローカルダッシュボード（`http://127.0.0.1:4318`）が開きます。
2. **AI接続**を開き、OpenAI、Anthropic Claude、xAI、Z.ai、またはローカルの ChatGPT (Codex) ログインを接続します。
3. ログインが必要なサイトでは、**アカウント → ログインを探す**をクリックしてブラウザーのセッションをローカルの暗号化保管庫に取り込みます。
4. ウェブサイトのURLと目的を入力してツールを作成するか、**コーディングエージェント**からMCP設定をコピーしてAIアシスタントに貼り付けます。

<a id="what-your-ai-can-do"></a>
## AIでできること

Hycliは実際のウェブサイトのインターフェースを、自己記述的なCLIサブコマンドとMCPツールに変換します:

| ウェブサイトのインターフェース | 準備される機能 |
| --- | --- |
| REST および OpenAPI | JSON/YAMLスキーマとAPIドキュメントに基づく型付きの読み取り・書き込みアクション |
| GraphQL | 型付きJSON変数を備えたクエリおよびミューテーション定義 |
| HTML および JS描画ページ | 観測されたセレクターによる一覧・リンク・ページネーション抽出、およびローカルChromiumエンジンによる描画済みGET読み取り |
| JSON および URLエンコードフォーム | ドキュメント化された入力、デフォルト値、アカウント単位のフォーム送信（`POST`、`PUT`、`PATCH`、`DELETE`） |

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="ワーカーが3人。とても忙しい小鳥が1羽。" width="100%" />
</p>
<p align="center"><sub>ワーカーが3人。とても忙しい小鳥が1羽。</sub></p>

| 操作の種類 | 実行時の動作 |
| --- | --- |
| **読み取り・検索** | 根拠が確認され、安全な読み取りに分類された場合は即座に実行します。 |
| **作成・送信・編集・削除** | 準備中に検出されCLI仕様に組み込まれますが、準備や検証中に実行されることはありません。デフォルトはオフ（`writes: false`）です。AIエージェントは **CLIごとに初回のみ** 書き込みを許可するか尋ね（`hycli writes SITE allow` または MCP `hycli_writes`）、許可された後は指示に応じて直接書き込みアクションを実行します。 |
| **影響が不明な操作** | 送信前に確認のため保留されます。 |
| **認証チャレンジ・レート制限** | 通信戦略を即座に切り替え（HTTP → ページ内 `window.fetch` → ステルスChromium）、または安全に一時停止します。 |

対応するリクエスト形式と抽出ルールについては[操作仕様](../SITESPEC.md)を参照してください。

<a id="common-concerns"></a>
## よくある懸念とHycliの解決策

AIエージェントを実際のウェブサイトやログイン済みアカウントに接続する際によくある懸念と、Hycliの設計による解決策です:

### 1. 「AIが勝手に投稿したり、アカウントのデータを編集・削除したりしませんか？」
- **CLI作成・点検中の実環境への変更はゼロ:** ウェブサイトの準備や確認中（`hycli prepare`、`hycli request`、`hycli recon`、`hycli check`）、Hycliは読み取りと書き込みの両方の操作を検出してCLI仕様にコンパイルしますが、**書き込みリクエストは一切実行しません**。読み取りを装った変更（`/delete` のような変更系パスへの `GET`、GraphQLの `mutation`、HTMLの `<form method="post">` 送信など）も検証実行から厳密にブロックされます。
- **CLIごとの初回1回のみの書き込み許可:** 新しく作成されたすべてのウェブサイトCLIは `writes: false` で始まります。AIエージェントがそのCLIで初めて書き込みアクションを実行する必要があるとき、**そのCLIにつき1回だけ** *「`<site>` の書き込みアクションを許可しますか？」* と確認します。あなたが承認して `hycli writes <site> allow`（または MCP `hycli_writes`）が有効になった後にのみ書き込みが直接実行されます。書き込み権限は `hycli writes <site> deny` またはダッシュボードの **変更を許可** トグルでいつでも取り消せます。
- **失敗した書き込みの自動再試行なし:** 失敗または中断された書き込みリクエストは決して自動再試行されません。送信後に接続が切れた場合、注文や投稿の重複を防ぐため `action_outcome_unknown` を返し、先にウェブサイト側を確認できるようにします。

### 2. 「Cookieやログイントークン、APIキーがAIモデル（LLM）に送信されませんか？」
- **秘密情報はLLMのプロンプトに一切入らない:** ブラウザーのCookie、`localStorage` / `sessionStorage` のトークン、APIキーはローカルの暗号化保管庫（Windows 資格情報マネージャー、macOS キーチェーン、Linux Secret Service、または `0600` ユーザー専用権限のファイル）にのみ厳重に保存されます。
- **シンボリック接続レシピ（`ConnectionRecipe`）:** AIモデルに渡されるのはアカウントラベル、ストレージのキー名、マスクされたレスポンス構造のみです。モデルはCookie名やトークン名を参照するシンボリックな `ConnectionRecipe` を組み立て、Hycliが実行時にローカルで実際の秘密情報を注入し、すべての出力とログから秘密情報を自動的に除去します。
- **サイト単位の読み取り専用インポート:** ブラウザーセッションの取り込みは一時的な読み取り専用スナップショットから対象ドメインのCookieとストレージのみを読み取り、ブラウザーのセキュリティを弱めたり他のサイトのデータを書き出したりすることはありません。

### 3. 「スクレイピングでCAPTCHAが出たり、アカウントが凍結されたりしませんか？」
- **ファジングや総当たりスキャンは一切なし:** Hycliはエンドポイントの単語リストを推測したり稼働中のサーバーにファジングを行ったりしません。リンクされたドキュメント、OpenAPI/GraphQLスキーマ、静的JSバンドル、観測されたブラウザーのネットワーク構造からのみ学習します。
- **直列化されたレート制限付き通信:** リクエストはウェブサイトおよびアカウント単位で直列化・速度制限され、`Retry-After` を尊重し、読み取りの再試行回数と応答サイズに上限を設けます。
- **戦略サーキットブレーカーと実ブラウザー実行:** 通常のHTTP通信が 403/429 やボット対策（Cloudflare、Akamai Bot Managerなど）に阻まれた場合、同じリクエストを繰り返しません。サーキットブレーカーが即座に通信方式を切り替え、認証済みブラウザーコンテキスト内の `window.fetch(..., { credentials: 'include' })` またはローカルのステルスChromiumエンジンで実行するか、安全に一時停止してアカウントを保護します。

### 4. 「リダイレクトでセッションが漏洩したり、共有した仕様ファイルからPCのパスが露出したりしませんか？」
- **厳格なオリジン束縛:** 認証ヘッダーとCookieは対象オリジンに固定され、別オリジンへのリダイレクト時には即座に削除されます。プライベートIPやループバックアドレスは `--allow-local` を明示しない限りデフォルトでブロックされます。
- **ポータブルな仕様ファイル:** ローカルのブラウザー実行ファイルやプロファイルのパスはマシン固有の状態にのみ保持され、共有可能な `SiteSpec` ファイルには一切書き込まれません。

### 5. 「ログインが切れたとき、AIがトークンを無駄遣いして再試行し続けませんか？」
- **AIを使わない決定論的ヘルスチェック（`hycli check`）:** `hycli check`（および MCP `hycli_check`）はLLMを呼び出さずに検証済みの読み取りチェーンを再生し、`signed_out`（ログイン切れ・再接続が必要）、`site_changed`（サイト構造の変更・仕様の修復が必要）、`blocked`、`unreachable` を明確に区別します。

<a id="ai-connections"></a>
## AI接続

| プロバイダー | 認証方式 |
| --- | --- |
| ChatGPT · APIキー | OpenAI APIキー |
| Claude | Anthropic APIキー |
| xAI | xAI APIキー |
| Z.ai / Z.ai Coding Plan | Z.ai APIキー |
| ChatGPT · ログイン | 公式 Codex app-server プロトコルによるローカル ChatGPT ログイン |

Codexログイン接続は、ファイル・シェル・ブラウザー・外部MCPの実行を無効化した一時スレッドで動作し、HycliがCodexのOAuthトークンを読み取ったりコピーしたりすることはありません。

<a id="use-hycli-with-your-ai"></a>
## AIでHycliを使う

お使いのMCPクライアントにHycliを追加します（**コーディングエージェント → 接続設定を見る** に現在のOS向けの正確なパスが表示されます）:

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

インストール済みのウェブサイトツールのみを公開するには `hycli mcp --sites-only` を使い、特定の1サイトに限定するには `--only-site SITE` を追加します。MCPサーバーは `hycli_prepare`、`hycli_writes`（ユーザーへの初回確認後にCLI単位で書き込みを有効化）、`hycli_run`、`hycli_check`、`hycli_result`、`hycli_activity` を提供し、認証情報を読み取るツールは一切公開しません。

ターミナルからも同じワークフローをそのまま利用できます:

```sh
hycli prepare https://your-website.example --intent "保存した参考資料を探す"
hycli request SITE "記事本文の編集、下書き保存、公開機能を追加して"
hycli describe SITE
hycli SITE ACTION --help
hycli writes SITE allow
hycli SITE ACTION --query "design systems"
hycli check SITE
```

[エージェントガイド](../../agent/AGENTS.md) · [Hycliスキル](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## ブラウザーでのログイン

HycliはWindows、macOS、Linux上のローカルのChrome、Chromium、Edge、Brave、Vivaldi、Arc、Whale、Firefoxのプロファイル（およびインストール済みのローカルステルスChromiumバイナリ）を検出し、指定されたウェブサイトのセッションのみをローカル保管庫に取り込みます。同じ検証済みウェブサイトアカウントを共有する複数のプロファイルは自動的に重複排除されます。

Windows App-Bound暗号化やプライベートコンテナー、未インポートのブラウザーによってセッションがロックされている場合は、**アカウント → アカウントを接続** からブラウザーでログインするか、[ブラウザー連携拡張機能](../../browser-companion/guide.html)の接続、またはダッシュボードでのJSON/Netscape Cookieファイルのインポートを利用してください（Cookieの内容をAIのチャット欄に直接貼り付けないでください）。

<a id="languages"></a>
## 言語

ダッシュボードおよびAIが生成するアクション説明は、右から左へ記述するアラビア語（RTL）を含め、このREADMEの上部にリンクされている20言語に対応しています。

<a id="local-data"></a>
## ローカルデータとリクエスト制限

ウェブサイト定義、暗号化された認証情報、アクティビティ履歴はOSのデータディレクトリ（Windowsは `%LOCALAPPDATA%\hycli`、macOSは `~/Library/Application Support/hycli`、Linuxは `~/.local/share/hycli`、または `HYCLI_DATA_DIR`）に保存されます。現在の保存先とキーリング保護の状態は **設定** からいつでも確認できます。

<a id="contributing"></a>
## 開発と検証

```sh
npm --prefix dashboard ci
npm --prefix dashboard run check
npm --prefix dashboard run build
cargo test --all-targets --locked
cargo build --locked --bin hycli --example dashboard_fixture
node scripts/test-e2e.mjs --full
```

すべてのテストは合成されたローカルウェブサイトとフィクスチャに対して実行されます。テスト目的で実際の外部サービスのアカウントに対して書き込み操作を実行しないでください。[画像の出典](../images/README.md)も参照してください。

## ライセンスと利用目的

Hycliは [Apache-2.0](../../LICENSE) ライセンスのもとで提供されています。アクセス権限のあるウェブサイトおよびアカウントに対してのみ使用してください。本ソフトウェアは保証なしの「現状有姿」で提供され、その使用に起因するいかなる損害、アカウントの制限や凍結についても著作者および貢献者は一切の責任を負いません。

## Star History

<a href="https://www.star-history.com/#Hybirdss/Hycli&Date">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date&theme=dark" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
 </picture>
</a>
