<p align="center">
  <img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="Biến trang web thành công cụ mà AI có thể sử dụng." width="100%" />
</p>

<p align="center">
  <strong>Biến mọi trang web thành công cụ CLI và MCP có kiểu dữ liệu rõ ràng cho tác nhân AI và terminal của bạn.</strong><br/>
  <sub>Đọc, tìm kiếm, tạo mới, chỉnh sửa và tự động hóa—tận dụng phiên đăng nhập trình duyệt thật của bạn, không bao giờ làm lộ cookie cho LLM và tích hợp sẵn khả năng chống chặn bot.</sub>
</p>

<p align="center">
  <a href="https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0">Tải xuống (Windows · macOS · Linux)</a> ·
  <a href="https://github.com/Hybirdss/Hycli/issues">Nhận trợ giúp trên GitHub</a> ·
  <a href="#try-it-in-30-seconds">Dùng thử trong 30 giây</a> ·
  <a href="#get-started">Bắt đầu</a> ·
  <a href="#what-your-ai-can-do">AI của bạn có thể làm gì</a> ·
  <a href="#common-concerns">Mối lo ngại thường gặp</a> ·
  <a href="#use-hycli-with-your-ai">Dùng với AI</a>
</p>

<details>
<summary>Đọc bằng ngôn ngữ của bạn · 20 ngôn ngữ</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

Chỉ cần cung cấp cho Hycli địa chỉ trang web bất kỳ và mục tiêu bạn muốn thực hiện. Ba tác nhân AI sẽ phân tích tài liệu, gói JS giao diện và lưu lượng mạng trình duyệt thực tế để xây dựng bộ công cụ CLI và MCP có kiểu dữ liệu—bao gồm cả thao tác đọc lẫn ghi—mà không hề gửi bất kỳ yêu cầu ghi thực tế nào vào tài khoản của bạn trong suốt quá trình chuẩn bị.

<p align="center">
  <img src="../../docs/images/dashboard.png" alt="Bảng điều khiển Hycli với thẻ trang web, thao tác khả dụng và tiến độ tác vụ." width="100%" />
</p>
<p align="center"><sub>Quản lý trang web, tài khoản, quyền ghi và kết quả thực thi tại một nơi trong bảng điều khiển cục bộ và CLI.</sub></p>

<a id="try-it-in-30-seconds"></a>
## Dùng thử trong 30 giây

```sh
# 1. Trỏ Hycli tới bất kỳ trang web nào và mô tả mục tiêu (chuẩn bị cả đọc & ghi; 0 thay đổi thực tế khi chuẩn bị)
hycli prepare https://your-website.example --intent "Tìm kiếm bài viết, lưu bản nháp và đăng bài mới"

# 2. Xem toàn bộ lệnh đọc, ghi và quy trình nhiều bước đã được chuẩn bị
hycli describe your-website

# 3. Chạy ngay các thao tác đọc và tìm kiếm
hycli your-website search-posts --query "ghi chú phát hành"

# 4. Khi cần tạo, sửa hoặc xóa, chỉ cần bật quyền ghi một lần duy nhất cho CLI này rồi chạy trực tiếp
hycli writes your-website allow
hycli your-website create-draft --title "Xin chào từ Hycli" --body "Tự động hóa trực tiếp từ terminal."
```

Hoặc kết nối Hycli với **Claude Code, Cursor, Codex, hoặc Windsurf** một lần bằng `"command": "hycli", "args": ["mcp"]` rồi yêu cầu tác nhân bằng ngôn ngữ tự nhiên:
> *"Dùng Hycli chuẩn bị `https://...`, cho tôi biết những thao tác nào dùng được và soạn giúp một bài viết nháp mới."*

<a id="get-started"></a>
## Bắt đầu

Hycli được đóng gói thành một tệp thực thi gốc duy nhất chứa cả bộ máy xử lý trang web lẫn bảng điều khiển cục bộ (`http://127.0.0.1:4318`). Bạn không cần cài đặt Rust hay Node.js để chạy gói phát hành.

### Tải xuống và chạy (Windows, macOS, Linux)

Tải bộ cài đặt gốc hoặc gói nén di động (portable) cho hệ điều hành của bạn từ **[GitHub Releases (v0.1.0)](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)**:

| Hệ điều hành | Kiến trúc | Bộ cài đặt gốc | Gói nén di động | Cách khởi chạy |
| --- | --- | --- | --- | --- |
| **Windows 10/11** | x64 & ARM64 | `hycli-0.1.0-win32-*-setup.exe` | `hycli-0.1.0-win32-*.tar.gz` | Chạy `*-setup.exe` (không cần quyền quản trị, tự thêm `hycli` vào `PATH` người dùng và Start Menu), sau đó mở **Hycli** hoặc chạy `hycli open` trong PowerShell |
| **macOS 13+** | Apple Silicon (`arm64`) & Intel (`x64`) | `hycli-0.1.0-darwin-*.dmg` | `hycli-0.1.0-darwin-*.tar.gz` | Mở tệp `.dmg`, kéo **Hycli.app** vào **Applications** rồi mở ứng dụng (tự động liên kết `~/.local/bin/hycli`) |
| **Linux** | x64 & ARM64 | `hycli-0.1.0-linux-*.deb` | `hycli-0.1.0-linux-*.tar.gz` | Cài đặt tệp `.deb` (`sudo apt install ./hycli-*.deb`) hoặc giải nén `.tar.gz` và chạy `./install.sh`, sau đó chạy `hycli open` |

<details>
<summary><strong>Cài đặt gói di động qua terminal & biên dịch từ mã nguồn</strong></summary>

**Gói di động cho Linux / macOS:**

```sh
sha256sum -c hycli-0.1.0-*.tar.gz.sha256   # hoặc trên macOS: shasum -a 256 -c
tar -xzf hycli-0.1.0-*.tar.gz
cd hycli-0.1.0-*
./install.sh
hycli open
```

**Gói di động cho Windows PowerShell:**

```powershell
tar -xzf hycli-0.1.0-win32-x64.tar.gz
cd hycli-0.1.0-win32-x64
powershell -ExecutionPolicy Bypass -File .\install.ps1
hycli open
```

**Biên dịch từ mã nguồn (Linux, macOS, Windows):**

Cài đặt [các công cụ cần thiết để biên dịch](../BUILD.md) rồi chạy:

```sh
node scripts/build.mjs
./dist/hycli open
```

Chạy `node scripts/package-desktop.mjs` để tạo bộ cài đặt gốc `.exe`, `.dmg` hoặc `.deb` cho hệ điều hành hiện tại. Xem thêm [hướng dẫn biên dịch và đóng gói](../BUILD.md) cùng [kiểm chứng bản phát hành](../RELEASING.md).

</details>

### Kết nối trang web đầu tiên của bạn

1. Chạy `hycli open` (hoặc nhấp vào biểu tượng ứng dụng **Hycli**) để mở bảng điều khiển cục bộ tại `http://127.0.0.1:4318`.
2. Mở **Kết nối AI** và kết nối OpenAI, Anthropic Claude, xAI, Z.ai hoặc tài khoản đăng nhập ChatGPT (Codex) cục bộ.
3. Đối với các trang web cần đăng nhập, nhấp **Tài khoản → Tìm phiên đăng nhập của tôi** để nhập phiên trình duyệt vào kho lưu trữ mã hóa cục bộ.
4. Nhập địa chỉ trang web và mục tiêu để tạo công cụ, hoặc sao chép cấu hình MCP từ mục **Tác nhân lập trình** vào trợ lý AI của bạn.

<a id="what-your-ai-can-do"></a>
## AI của bạn có thể làm gì

Hycli biến các giao diện trang web thực tế thành các lệnh con CLI và công cụ MCP có tài liệu mô tả rõ ràng:

| Giao diện trang web | Khả năng được chuẩn bị |
| --- | --- |
| REST và OpenAPI | Các thao tác đọc và ghi có kiểu dữ liệu dựa trên lược đồ JSON/YAML và tài liệu API. |
| GraphQL | Các tài liệu truy vấn (query) và thay đổi (mutation) đã phân tích cú pháp cùng biến JSON có kiểu dữ liệu. |
| Trang HTML & kết xuất bằng JS | Trích xuất danh sách bản ghi, liên kết, phân trang theo bộ chọn quan sát được và đọc trang GET kết xuất qua bộ máy Chromium cục bộ. |
| Biểu mẫu JSON và mã hóa URL | Đầu vào có tài liệu mô tả, giá trị mặc định và gửi biểu mẫu theo phạm vi tài khoản (`POST`, `PUT`, `PATCH`, `DELETE`). |

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="Ba tác nhân. Một chú chim bé xíu bận rộn hết sức." width="100%" />
</p>
<p align="center"><sub>Ba tác nhân. Một chú chim bé xíu bận rộn hết sức.</sub></p>

| Loại thao tác | Hành vi thực thi |
| --- | --- |
| **Đọc hoặc tìm kiếm** | Chạy ngay lập tức khi có bằng chứng xác thực và được phân loại là thao tác đọc an toàn. |
| **Tạo, gửi, sửa hoặc xóa** | Được phát hiện và tổng hợp vào đặc tả CLI nhưng tuyệt đối không bao giờ chạy trong quá trình chuẩn bị hay kiểm tra. Mặc định tắt (`writes: false`). Tác nhân AI sẽ hỏi bạn **một lần duy nhất cho mỗi CLI** xem có cho phép ghi hay không (`hycli writes SITE allow` hoặc MCP `hycli_writes`). Sau khi bật, các thao tác ghi sẽ chạy trực tiếp khi bạn yêu cầu. |
| **Tác động chưa rõ ràng** | Được giữ lại để xem xét trước khi gửi yêu cầu. |
| **Thử thách xác thực hoặc giới hạn tốc độ** | Chuyển đổi ngay chiến lược truyền tải (HTTP → `window.fetch` trong trang → Chromium ẩn danh) hoặc tạm dừng an toàn. |

Xem [đặc tả thao tác](../SITESPEC.md) để biết các dạng yêu cầu và quy tắc trích xuất được hỗ trợ.

<a id="common-concerns"></a>
## Mối lo ngại thường gặp và cách Hycli giải quyết

Khi kết nối tác nhân AI với các trang web thực tế và tài khoản đang đăng nhập, người dùng thường có những lo ngại chính đáng. Dưới đây là cách Hycli giải quyết từng vấn đề ngay từ thiết kế:

### 1. "Liệu AI có vô tình đăng bài, chỉnh sửa hay xóa dữ liệu trên tài khoản của tôi không?"
- **Không có bất kỳ thao tác ghi thực tế nào khi tạo và kiểm tra CLI:** Trong suốt quá trình chuẩn bị hoặc kiểm tra trang web (`hycli prepare`, `hycli request`, `hycli recon`, `hycli check`), Hycli phát hiện và biên dịch cả thao tác đọc lẫn ghi vào đặc tả CLI, nhưng **tuyệt đối không gửi yêu cầu ghi tới máy chủ**. Ngay cả các thao tác thay đổi ngụy trang dưới dạng đọc (đường dẫn `GET` có chứa `/delete`, thân yêu cầu GraphQL `mutation`, hoặc biểu mẫu HTML `<form method="post">`) cũng bị chặn hoàn toàn khi chạy xác minh.
- **Xác nhận quyền ghi một lần duy nhất cho mỗi CLI:** Mọi CLI trang web mới tạo đều bắt đầu ở trạng thái tắt ghi (`writes: false`). Khi tác nhân AI cần chạy thao tác ghi trên một CLI lần đầu tiên, nó sẽ hỏi bạn **một lần duy nhất cho CLI đó**: *"Cho phép thực hiện thao tác ghi trên `<site>`?"* Chỉ sau khi bạn đồng ý để bật `hycli writes <site> allow` (hoặc MCP `hycli_writes`), các thao tác ghi mới được thực thi trực tiếp. Bạn có thể thu hồi quyền ghi bất cứ lúc nào bằng `hycli writes <site> deny` hoặc nút gạt **Cho phép thay đổi** trên bảng điều khiển.
- **Không tự động thử lại thao tác ghi bị lỗi:** Các yêu cầu ghi bị lỗi hoặc bị gián đoạn không bao giờ được tự động gửi lại. Nếu mất kết nối sau khi gửi yêu cầu thay đổi, Hycli trả về `action_outcome_unknown` để bạn kiểm tra trang web trước thay vì tạo trùng đơn hàng hay bài đăng.

### 2. "Hycli có gửi cookie, token đăng nhập hay khóa API của tôi cho mô hình AI (LLM) không?"
- **Thông tin bí mật không bao giờ đi vào prompt của LLM:** Cookie trình duyệt, token `localStorage` / `sessionStorage` và khóa API chỉ được lưu trữ nghiêm ngặt trong kho mã hóa cục bộ trên máy bạn (mã hóa qua trình quản lý khóa của hệ điều hành như Windows Credential Manager, macOS Keychain, Linux Secret Service, hoặc tệp chỉ dành cho người dùng hiện tại với quyền `0600`).
- **Công thức kết nối ký hiệu (`ConnectionRecipe`):** Mô hình AI chỉ nhìn thấy nhãn tài khoản, tên khóa lưu trữ và lược đồ phản hồi đã được che thông tin nhạy cảm. Mô hình xây dựng một `ConnectionRecipe` dạng ký hiệu (chỉ tham chiếu đến tên cookie hoặc token), và Hycli sẽ chèn giá trị bí mật thực tế ngay tại máy cục bộ khi thực thi đồng thời tự động xóa mọi thông tin bí mật khỏi kết quả đầu ra và nhật ký.
- **Nhập phiên chỉ đọc theo đúng phạm vi trang web:** Việc nhập phiên trình duyệt chỉ đọc cookie và bộ nhớ của đúng tên miền mục tiêu từ một bản chụp chỉ đọc tạm thời, không bao giờ làm giảm mức bảo mật của trình duyệt hay xuất dữ liệu của các trang web khác.

### 3. "Việc thu thập dữ liệu có làm xuất hiện CAPTCHA hoặc khiến tài khoản của tôi bị khóa không?"
- **Không fuzzing hay quét vét cạn (brute-force):** Hycli không bao giờ đoán danh sách đường dẫn ẩn hay kiểm thử fuzzing lên máy chủ đang hoạt động. Nó chỉ học từ tài liệu được liên kết, lược đồ OpenAPI/GraphQL, các gói JS tĩnh và cấu trúc mạng trình duyệt thực tế quan sát được.
- **Lưu lượng tuần tự, được kiểm soát tốc độ:** Các yêu cầu được xếp hàng tuần tự và giới hạn tốc độ theo từng trang web và từng tài khoản, tuân thủ tiêu đề `Retry-After` cũng như giới hạn chặt chẽ số lần thử lại thao tác đọc và kích thước phản hồi.
- **Bộ ngắt mạch chiến lược & thực thi trong ngữ cảnh trình duyệt thật:** Khi yêu cầu HTTP thông thường gặp lỗi 403/429 hoặc thử thách chống bot (Cloudflare, Akamai Bot Manager), Hycli không bao giờ lặp lại yêu cầu đã bị chặn. Bộ ngắt mạch sẽ lập tức chuyển đổi phương thức truyền tải—thực thi `window.fetch(..., { credentials: 'include' })` ngay bên trong ngữ cảnh trình duyệt đã xác thực của bạn hoặc dùng bộ máy Chromium ẩn danh cục bộ—hoặc tạm dừng an toàn để bảo vệ tài khoản.

### 4. "Chuyển hướng (redirect) có thể làm lộ phiên của tôi, hoặc việc chia sẻ tệp đặc tả có làm lộ đường dẫn máy tính không?"
- **Ràng buộc nguồn gốc (Origin) nghiêm ngặt:** Các tiêu đề xác thực và cookie bị khóa chặt vào nguồn gốc mục tiêu và bị loại bỏ ngay lập tức khi chuyển hướng sang tên miền khác. Các dải IP mạng riêng và cục bộ (loopback) bị chặn mặc định trừ khi chỉ định rõ `--allow-local`.
- **Đặc tả di động an toàn:** Đường dẫn tệp thực thi trình duyệt và hồ sơ cục bộ chỉ nằm trong trạng thái riêng của máy và không bao giờ được ghi vào các tệp `SiteSpec` dùng để chia sẻ.

### 5. "Khi phiên đăng nhập hết hạn, AI có tốn token để thử lại liên tục không?"
- **Kiểm tra trạng thái xác định không cần AI (`hycli check`):** `hycli check` (và MCP `hycli_check`) phát lại chuỗi thao tác đọc đã xác minh mà không cần gọi LLM, giúp phân biệt rõ ràng giữa `signed_out` (phiên đăng nhập hết hạn, cần kết nối lại tài khoản) với `site_changed` (cấu trúc trang web đã thay đổi, cần sửa đặc tả), `blocked` và `unreachable`.

<a id="ai-connections"></a>
## Kết nối AI

| Nhà cung cấp | Phương thức xác thực |
| --- | --- |
| ChatGPT · khóa API | Khóa API OpenAI |
| Claude | Khóa API Anthropic |
| xAI | Khóa API xAI |
| Z.ai / Z.ai Coding Plan | Khóa API Z.ai |
| ChatGPT · đăng nhập | Đăng nhập ChatGPT cục bộ qua giao thức Codex app-server chính thức |

Kết nối đăng nhập Codex chạy trong một luồng tạm thời đã vô hiệu hóa quyền thực thi tệp, shell, trình duyệt và MCP bên ngoài; Hycli không bao giờ đọc hay sao chép token OAuth của Codex.

<a id="use-hycli-with-your-ai"></a>
## Dùng Hycli với AI của bạn

Thêm Hycli vào máy khách MCP của bạn (**Tác nhân lập trình → Xem cài đặt kết nối** sẽ hiển thị đường dẫn chính xác cho hệ điều hành của bạn):

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

Dùng `hycli mcp --sites-only` nếu chỉ muốn hiển thị các công cụ trang web đã cài đặt, hoặc thêm `--only-site SITE` để giới hạn trong một trang web duy nhất. Máy chủ MCP cung cấp `hycli_prepare`, `hycli_writes` (bật quyền ghi một lần cho mỗi CLI sau khi hỏi bạn), `hycli_run`, `hycli_check`, `hycli_result` và `hycli_activity`—và tuyệt đối không bao giờ cung cấp công cụ nào để đọc thông tin xác thực.

Từ terminal, mọi quy trình đều hoạt động theo cùng một cách:

```sh
hycli prepare https://your-website.example --intent "Tìm tài liệu tham khảo đã lưu"
hycli request SITE "Thêm tính năng chỉnh sửa nội dung, lưu bản nháp và đăng bài viết"
hycli describe SITE
hycli SITE ACTION --help
hycli writes SITE allow
hycli SITE ACTION --query "design systems"
hycli check SITE
```

[Hướng dẫn tác nhân](../../agent/AGENTS.md) · [Skill Hycli](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## Đăng nhập qua trình duyệt

Hycli tự động phát hiện các hồ sơ Chrome, Chromium, Edge, Brave, Vivaldi, Arc, Whale và Firefox cục bộ (cùng các tệp thực thi Chromium ẩn danh cục bộ nếu đã cài đặt) trên Windows, macOS và Linux, chỉ nhập phiên của đúng trang web được yêu cầu vào kho lưu trữ mã hóa cục bộ. Các hồ sơ dùng chung cùng một tài khoản trang web đã xác minh sẽ tự động được gộp lại.

Nếu phiên bị khóa bởi cơ chế mã hóa Windows App-Bound, vùng chứa riêng tư (container) hoặc trình duyệt chưa được hỗ trợ nhập trực tiếp, hãy vào **Tài khoản → Kết nối tài khoản** để đăng nhập qua trình duyệt, kết nối [tiện ích mở rộng đồng hành trên trình duyệt](../../browser-companion/guide.html), hoặc nhập tệp cookie JSON/Netscape trong bảng điều khiển (tuyệt đối không dán nội dung cookie trực tiếp vào cửa sổ trò chuyện AI).

<a id="languages"></a>
## Ngôn ngữ

Bảng điều khiển và phần mô tả thao tác do AI tạo ra hỗ trợ 20 ngôn ngữ được liên kết ở đầu tệp README này, bao gồm cả tiếng Ả Rập viết từ phải sang trái (RTL).

<a id="local-data"></a>
## Dữ liệu cục bộ và giới hạn yêu cầu

Định nghĩa trang web, thông tin xác thực đã mã hóa và nhật ký hoạt động được lưu trong thư mục dữ liệu của hệ điều hành (`%LOCALAPPDATA%\hycli` trên Windows, `~/Library/Application Support/hycli` trên macOS, `~/.local/share/hycli` trên Linux, hoặc `HYCLI_DATA_DIR`). Bạn có thể kiểm tra đường dẫn hiện tại và trạng thái bảo vệ kho khóa bất cứ lúc nào trong mục **Cài đặt**.

<a id="contributing"></a>
## Phát triển và kiểm chứng

```sh
npm --prefix dashboard ci
npm --prefix dashboard run check
npm --prefix dashboard run build
cargo test --all-targets --locked
cargo build --locked --bin hycli --example dashboard_fixture
node scripts/test-e2e.mjs --full
```

Tất cả các bài kiểm thử đều chạy trên các trang web cục bộ mô phỏng và dữ liệu giả lập (fixtures)—tuyệt đối không kiểm thử thao tác ghi trên tài khoản dịch vụ bên thứ ba thật. Xem thêm [nguồn hình ảnh](../images/README.md).

## Giấy phép và mục đích sử dụng

Hycli được phát hành theo giấy phép [Apache-2.0](../../LICENSE). Chỉ sử dụng phần mềm với các trang web và tài khoản mà bạn có quyền truy cập hợp pháp. Phần mềm được cung cấp "nguyên trạng" không kèm theo bảo đảm; các tác giả và người đóng góp không chịu trách nhiệm đối với bất kỳ tổn thất, hạn chế tài khoản hoặc việc khóa tài khoản nào phát sinh từ quá trình sử dụng.

## Star History

<a href="https://www.star-history.com/#Hybirdss/Hycli&Date">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date&theme=dark" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
 </picture>
</a>
