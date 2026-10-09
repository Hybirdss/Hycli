<p align="center">
  <img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="Biến trang web thành công cụ mà AI có thể sử dụng." width="100%" />
</p>

<p align="center">
  <strong>Biến trang web thành công cụ mà AI có thể sử dụng.</strong>
</p>

<p align="center">
  <a href="#get-started">Bắt đầu</a> ·
  <a href="#what-your-ai-can-do">AI của bạn có thể làm gì</a> ·
  <a href="#ai-connections">Kết nối AI</a> ·
  <a href="#use-hycli-with-your-ai">Dùng với AI</a>
</p>

<details>
<summary>Đọc bằng ngôn ngữ của bạn · 20 ngôn ngữ</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

Thêm một trang web. Hycli chuẩn bị các thao tác mà AI có thể sử dụng và giải thích công dụng của từng thao tác. Quản lý trang web, thông tin đăng nhập và kết quả tại một nơi trong bảng điều khiển cục bộ.

| Kết nối | Chuẩn bị | Sử dụng |
| --- | --- | --- |
| Thêm trang web và chọn AI. | Theo dõi tiến độ và xem các thao tác khả dụng. | Chạy một thao tác hoặc giao cho trợ lý AI. |

<p align="center">
  <img src="../../docs/images/dashboard.png" alt="Bảng điều khiển Hycli với thẻ trang web, thao tác khả dụng và tiến độ tác vụ." width="100%" />
</p>
<p align="center"><sub>Không gian làm việc mẫu với các công cụ và tài khoản mẫu.</sub></p>

<a id="get-started"></a>
## Bắt đầu

Gói ứng dụng mới: Windows (`*-setup.exe`), macOS (`.dmg`) và Linux (`.deb` hoặc chạy `./install.sh` sau khi giải nén). Mở Hycli bằng biểu tượng: bộ máy tự chạy nền và mở trình duyệt. Không cần giữ cửa sổ terminal. Thoát trong Cài đặt hoặc dùng `hycli open`, `hycli status`, `hycli stop`. Các gói đang phát triển này tách biệt với v0.1.0; xem kiểm chứng nền tảng và chữ ký trong [hướng dẫn](../BUILD.md) và [CI](https://github.com/Hybirdss/Hycli/actions/workflows/verify.yml).

**[Tải xuống v0.1.0 · Linux x64](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)** · glibc ≥ 2.39 · [SHA-256](https://github.com/Hybirdss/Hycli/releases/download/v0.1.0/hycli-0.1.0-linux-x64.tar.gz.sha256)

Hycli là ứng dụng Rust có bảng điều khiển web chạy cục bộ. Từ bản mã nguồn đã tải về này, hãy biên dịch bảng điều khiển và tệp thực thi:

```sh
node scripts/build.mjs
./dist/hycli open
```

Bảng điều khiển mở tại `http://127.0.0.1:4318`. Dùng `hycli dashboard --no-open` để in địa chỉ mà không mở trình duyệt, hoặc `--port 4320` để chọn cổng khác. Bộ máy xử lý trang web và bảng điều khiển nằm trong cùng một tệp thực thi.

1. Mở **Kết nối AI** và kết nối nhà cung cấp.
2. Thêm địa chỉ trang web và chọn AI sẽ chuẩn bị trang đó.
3. Xem các thao tác khả dụng. Chạy thao tác trong bảng điều khiển hoặc mở **Tác nhân lập trình** để sao chép cấu hình MCP một lần.

Với trang web cần đăng nhập, hãy kết nối tài khoản từ mục **Tài khoản**. Một trang web có thể lưu nhiều tài khoản; bạn chọn tài khoản mà các công cụ sẽ sử dụng.

<a id="what-your-ai-can-do"></a>
## AI của bạn có thể làm gì

Hycli chuyển các thao tác được hỗ trợ trên trang web thành công cụ có tên và kiểu dữ liệu. Phần mô tả do AI viết giải thích mục đích của mỗi thao tác, thông tin cần cung cấp và kết quả trả về.

Ba tác nhân AI độc lập lần lượt xử lý việc đọc và tìm kiếm, các quy trình hữu ích và bằng chứng về tài khoản. Bảng điều khiển hiển thị các giai đoạn đã hoàn tất, trạng thái của từng tác nhân, thời gian đã trôi qua và ghi chú công việc dễ hiểu. Công việc qua CLI và MCP cũng xuất hiện trong cùng lịch sử hoạt động.

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="Ba tác nhân. Một chú chim bé xíu bận rộn hết sức." width="100%" />
</p>
<p align="center"><sub>Ba tác nhân. Một chú chim bé xíu bận rộn hết sức.</sub></p>

Quá trình chuẩn bị dựa trên thông tin thực sự có trên trang web: tài liệu được liên kết, lược đồ API, JavaScript công khai và cấu trúc yêu cầu mà tiện ích trình duyệt quan sát được. Hycli có thể đọc trong phạm vi giới hạn để kiểm tra một thao tác. Nó không tạo bản ghi thử nghiệm, sửa nội dung, xóa đối tượng, phỏng đoán hàng loạt điểm cuối hay kiểm thử fuzz trên máy chủ đang hoạt động.

| Thao tác | Hành vi |
| --- | --- |
| Đọc hoặc tìm kiếm | Tự thực hiện khi có bằng chứng hỗ trợ và được phân loại là thao tác đọc. |
| Tạo, gửi, sửa hoặc xóa | Hiển thị trang web, tài khoản, thao tác và các giá trị đầu vào đã xác định để người dùng phê duyệt. |
| Tác động chưa rõ | Cần xem xét trước khi gửi yêu cầu. |
| Xác thực, thử thách xác minh hoặc giới hạn yêu cầu | Tạm dừng các yêu cầu bị ảnh hưởng để bạn kết nối lại hoặc chờ. |

Mỗi lần phê duyệt chỉ áp dụng cho đúng một yêu cầu, hết hạn sau năm phút và chỉ dùng được một lần. Thay đổi đầu vào, tài khoản, thông tin đăng nhập đã lưu hoặc định nghĩa công cụ đã cài sẽ làm mất hiệu lực phê duyệt. Việc thực thi qua CLI, MCP và bảng điều khiển tuân theo cùng một ranh giới. Thao tác ghi không bao giờ được tự động thử lại.

Mức độ hỗ trợ tùy thuộc vào trang web. Trang không có tài liệu hữu ích hoặc thao tác đã quan sát được có thể cần trình duyệt đã đăng nhập, SiteSpec được cung cấp hoặc bước chuẩn bị bổ sung. Hycli báo rõ những gì nó hỗ trợ thay vì khẳng định mọi trang web đều có API sẵn dùng.

### Phạm vi hỗ trợ

Hỗ trợ OpenAPI dạng JSON hoặc YAML, REST, GraphQL, nội dung JSON và biểu mẫu mã hóa URL. Có thể trích xuất bản ghi, liên kết và trang tiếp theo từ HTML bằng bộ chọn đã quan sát; cũng hỗ trợ đọc trang GET được Chromium cục bộ kết xuất. Nếu lần đọc trong quá trình chuẩn bị thất bại, AI xem xét bằng chứng, sửa định nghĩa và kiểm tra lại.

Chưa hỗ trợ chuỗi nhấp trình duyệt tùy ý, tải lên multipart hoặc tải xuống tệp nhị phân. Gói phân phối không chứa tài khoản hay CLI đã tạo trước đó.

[Đặc tả thao tác](../SITESPEC.md) · [Kiểm tra gói](../RELEASING.md)

<a id="ai-connections"></a>
## Kết nối AI

| Kết nối | Đăng nhập |
| --- | --- |
| ChatGPT · khóa API | Khóa API OpenAI của bạn |
| Claude | Khóa API Anthropic của bạn |
| xAI | Khóa API xAI của bạn |
| Z.ai | Khóa API Z.ai của bạn |
| Z.ai Coding Plan | Khóa API Z.ai Coding Plan của bạn |
| ChatGPT · đăng nhập | Đăng nhập ChatGPT do Codex app-server đã cài đặt quản lý |

Chọn mô hình trong bảng điều khiển, bao gồm cả mô hình chỉ dành cho tài khoản của bạn. Kiểm tra kết nối xác minh nhà cung cấp và mô hình đã chọn. Nhà cung cấp API tính phí sử dụng vào tài khoản của bạn tại nhà cung cấp đó.

Kết nối Codex dùng giao thức app-server chính thức. Hycli không đọc hay sao chép token OAuth của Codex. Suy luận chạy trong luồng tạm thời, với khả năng thực thi tệp, shell, trình duyệt, ứng dụng và MCP bị vô hiệu hóa. Các công cụ đọc do Hycli kiểm soát thực hiện việc chuẩn bị trang web.

<a id="use-hycli-with-your-ai"></a>
## Dùng Hycli với AI

**Tác nhân lập trình → Xem cài đặt kết nối**

Kết nối MCP chung hỗ trợ cả chuẩn bị trang web và thực thi thao tác. Kết nối tác nhân một lần để tác nhân có thể chuẩn bị công cụ còn thiếu, theo dõi tiến độ và dùng thao tác mới.

```sh
hycli mcp
```

Dùng `--sites-only` để chỉ cung cấp các thao tác đã cài. Kết nối dưới đây được giới hạn trong một trang web.

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

Nếu PATH của tác nhân không có `hycli`, hãy dùng đường dẫn tệp thực thi hiển thị trên bảng điều khiển.

CLI cũng hỗ trợ cùng quy trình. Thay URL mẫu, `SITE` và `ACTION` bằng trang web của bạn và các tên do `describe` trả về.

```sh
hycli prepare https://your-website.example --intent "Tìm tài liệu tham khảo đã lưu"
hycli request SITE "Add draft editing and publishing with full post content"
hycli describe
hycli describe SITE ACTION
hycli SITE ACTION --help
hycli run SITE ACTION --arg query="design systems"
hycli jobs show JOB_ID --watch
```

Với MCP, gửi URL và `intent` cho `hycli_prepare`, rồi theo dõi bằng `hycli_job` hoặc `hycli_result`. `hycli_run` có thể chạy thao tác mới trước khi máy khách làm mới danh sách công cụ. Hãy tìm ID nội bộ bằng các thao tác liệt kê hoặc tìm kiếm liên quan trước.

Thao tác thay đổi trả về một phiếu để duyệt trên bảng điều khiển. Theo dõi kết quả của phiếu đó thay vì gửi lại yêu cầu. Lần đọc thất bại hoặc phản hồi không như dự kiến cũng tạo trạng thái lỗi trong CLI.

[Hướng dẫn tác nhân](../../agent/AGENTS.md) · [Skill Hycli](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## Đăng nhập qua trình duyệt

Quá trình chuẩn bị trang web bắt đầu bằng việc kiểm tra hệ điều hành hiện tại, các trình duyệt đang chạy hoặc đã đăng ký và các hồ sơ hiện có. AI chọn trong số những khả năng đó để đọc tài liệu, hiển thị trang, nhập phiên của trang web và xác minh kết nối. Đường dẫn trình duyệt và hồ sơ chỉ ở trên máy cục bộ; định nghĩa trang web được tạo ra không phụ thuộc vào đường dẫn cài đặt của nhà phát triển.

Có thể nhập cookie và dữ liệu lưu trữ theo nguồn từ Chrome, Chromium, Edge, Brave và Firefox khi truy cập được. Chỉ phiên của trang web được yêu cầu mới vào kho cục bộ của Hycli. Hycli tự động chọn một danh tính đã xác minh; các hồ sơ thuộc cùng danh tính đó được tính là một lựa chọn tài khoản. Lựa chọn tài khoản hiện tại được giữ nguyên; các danh tính đã xác minh khác nhau cần được chọn.

Trong **Tài khoản → Kết nối tài khoản**, chọn **Tìm phiên đăng nhập của tôi** để tìm lại. Nếu không tìm thấy phiên có thể dùng, chọn **Mở trang đăng nhập của website** để mở trang web trong trình duyệt mặc định. Hycli kiểm tra lại khi hộp thoại còn mở và chỉ tiếp tục chuẩn bị sau khi tài khoản được chọn đã xác minh. Chỉ mở một thẻ không được coi là đăng nhập thành công.

Phiên được hệ điều hành bảo vệ, gắn với trình duyệt, phiên riêng tư hoặc phiên trong vùng chứa có thể cần tiện ích trình duyệt. Chọn thẻ tiện ích, tạo mã kết nối, rồi mở tiện ích trong hồ sơ đã đăng nhập, nhập mã và cấp quyền truy cập trang web đó. Việc nhập trực tiếp không vượt qua cơ chế bảo vệ Windows App-Bound hoặc làm yếu tính bảo mật của hồ sơ trình duyệt.

- **Chrome và Edge:** giải nén gói và dùng **Tải tiện ích đã giải nén** trên trang tiện ích mở rộng của trình duyệt.
- **Firefox:** dùng gói Firefox và chọn **Tải tiện ích tạm thời** trong `about:debugging`. Tiện ích tạm thời bị xóa khi Firefox khởi động lại. Bản phát hành này không bao gồm bản phân phối có chữ ký qua cửa hàng.
- **Tệp cookie:** có thể nhập tệp cookie định dạng JSON và Netscape làm phương án dự phòng. Nhập tệp trực tiếp trong bảng điều khiển; không dán nội dung vào cuộc trò chuyện với AI.

Trình duyệt chuyển dữ liệu phiên trực tiếp vào kho thông tin xác thực cục bộ. Mô hình nhận nhãn tài khoản, tên và cấu trúc khóa cục bộ, cùng kết quả công cụ đã loại bỏ giá trị xác thực. Mô hình có thể tạo công thức kết nối tham chiếu các giá trị cục bộ đó mà không nhận chúng. Phạm vi, đường dẫn, thời hạn và thông tin phân vùng cookie được hỗ trợ đều được tôn trọng; tiêu đề xác thực chỉ được dùng với nguồn gốc ban đầu.

Danh tính tài khoản được xác định từ phản hồi thực tế về tài khoản hiện tại của trang web. Tên hồ sơ trình duyệt không được coi là danh tính trang web đã xác minh. Nếu chưa xác định được tài khoản, Hycli sẽ thông báo rõ. Sau khi kết nối, việc duyệt web bình thường có thể cung cấp phản hồi tài khoản và cấu trúc các yêu cầu khả dụng mà không tiết lộ giá trị yêu cầu, tiêu đề hay nội dung phản hồi cho mô hình chuẩn bị.

Một trang web có thể cần thông tin xác thực API riêng theo tài liệu hoặc khả năng trình duyệt không có sẵn cục bộ. Quá trình chuẩn bị ghi lại kết quả kết nối và đọc thực tế. Tải được trang chủ hoặc API trả về trang đăng nhập không có nghĩa là tích hợp đã sẵn sàng. [Hướng dẫn thiết lập trình duyệt](../../browser-companion/guide.html) giải thích quy trình tiện ích và các hạn chế.

<a id="languages"></a>
## Ngôn ngữ

Bảng điều khiển hỗ trợ 20 ngôn ngữ được liên kết ở trên, bao gồm tiếng Ả Rập viết từ phải sang trái. Ngôn ngữ ban đầu là tiếng Anh. Lựa chọn được lưu trên thiết bị này và mô tả AI có thể được chuẩn bị bằng ngôn ngữ đã chọn.

<a id="local-data"></a>
## Dữ liệu cục bộ và giới hạn yêu cầu

Định nghĩa trang web, siêu dữ liệu tài khoản và hoạt động được lưu trong thư mục dữ liệu cục bộ. Đặt `HYCLI_DATA_DIR` để chọn vị trí khác. Dùng **Cài đặt** để xem vị trí đang dùng và cách bảo vệ thông tin xác thực.

Khi có thể, kho thông tin xác thực dùng khóa mã hóa được bảo vệ bằng kho khóa của hệ điều hành. Khi không có kho khóa, hệ thống thông báo rõ rằng dữ liệu chỉ được bảo vệ bằng quyền truy cập tệp; thư mục riêng và tệp xác thực chỉ cho phép người dùng hệ điều hành hiện tại truy cập. Kho đã mã hóa không bị âm thầm thay thế khi không thể mở khóa.

Yêu cầu được xử lý tuần tự và điều tiết theo từng trang web và tài khoản, đồng thời có hạn mức bổ sung cho toàn trang web. Hycli tuân thủ `Retry-After`, giới hạn kích thước phản hồi và số lần thử lại thao tác đọc, đồng thời tạm dừng khi xác thực thất bại hoặc gặp thử thách xác minh của trang. Quá trình chuẩn bị trang web không luân phiên tài khoản, giả mạo dấu vân tay hay vượt thử thách để né hạn chế. Các biện pháp này giảm tải không cần thiết nhưng không đảm bảo trang web sẽ không bao giờ hạn chế tài khoản.

Địa chỉ trang web trong mạng riêng và trên máy cục bộ bị vô hiệu hóa mặc định. Với trang tự lưu trữ đáng tin cậy, hãy chỉ định rõ `--allow-local` khi khởi động bảng điều khiển hoặc máy chủ MCP.

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

Kiểm thử dùng trang web cục bộ và phản hồi nhà cung cấp được mô phỏng. Các kiểm thử bao gồm ràng buộc phê duyệt và ngăn phát lại, loại bỏ bí mật, chuyển hướng, giới hạn tốc độ, không thử lại thao tác ghi, bảo vệ phiên trình duyệt và hợp đồng phản hồi của nhà cung cấp. Không bao giờ dùng tài khoản thật để tạo, sửa hoặc xóa nội dung chỉ nhằm kiểm thử Hycli.

Ảnh chụp màn hình và GIF sử dụng dữ liệu mẫu riêng biệt. Xem [nguồn hình ảnh](../images/README.md).

## Giấy phép và mục đích sử dụng

Hycli được cấp phép theo [Apache-2.0](../../LICENSE).

Hycli dành cho việc tạo và sử dụng công cụ dòng lệnh giúp thao tác với trang web dễ dàng hơn. Chỉ sử dụng với trang web và tài khoản mà bạn được phép truy cập.

Phần mềm được cung cấp nguyên trạng, không có bảo hành. Trong phạm vi pháp luật cho phép, tác giả và người đóng góp không chịu trách nhiệm về tổn thất hay vấn đề khác phát sinh từ việc sử dụng. Bạn chịu trách nhiệm về cách mình sử dụng phần mềm.

Trong phạm vi pháp luật cho phép, tác giả và những người đóng góp không chịu trách nhiệm về việc tài khoản bị cấm, đình chỉ hoặc hạn chế do sử dụng Hycli. Không sử dụng Hycli để xâm nhập trái phép, truy cập khi chưa được phép hoặc thực hiện các cuộc tấn công.
