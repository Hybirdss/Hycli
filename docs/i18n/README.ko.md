<p align="center">
  <img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="웹사이트를 AI가 사용할 수 있는 도구로 바꾸세요." width="100%" />
</p>

<p align="center">
  <strong>웹사이트를 AI가 사용할 수 있는 도구로 바꾸세요.</strong>
</p>

<p align="center">
  <a href="#get-started">시작하기</a> ·
  <a href="#what-your-ai-can-do">AI로 할 수 있는 일</a> ·
  <a href="#ai-connections">AI 연결</a> ·
  <a href="#use-hycli-with-your-ai">AI와 함께 사용</a>
</p>

<details>
<summary>내 언어로 읽기 · 20개 언어</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

웹사이트를 추가하세요. Hycli가 AI에서 사용할 수 있는 작업을 준비하고 각 작업의 기능을 설명합니다. 웹사이트, 로그인 정보, 결과를 로컬 대시보드 한곳에서 관리하세요.

| 연결 | 준비 | 사용 |
| --- | --- | --- |
| 웹사이트를 추가하고 AI를 선택하세요. | 진행 상황과 사용 가능한 작업을 확인하세요. | 작업을 실행하거나 AI 어시스턴트에 연결하세요. |

<p align="center">
  <img src="../../docs/images/dashboard.png" alt="웹사이트 카드, 사용 가능한 작업, 작업 진행 상황을 보여주는 Hycli 대시보드." width="100%" />
</p>
<p align="center"><sub>샘플 도구와 계정으로 구성한 예시 화면입니다.</sub></p>

<a id="get-started"></a>
## 시작하기

새 데스크톱 패키지: Windows 설치 파일(`*-setup.exe`), macOS 앱(`.dmg`), Linux(`.deb` 또는 압축 해제 후 `./install.sh`). 설치 후 Hycli 아이콘을 누르면 백그라운드 엔진과 브라우저가 자동으로 열립니다. 터미널을 계속 켜 둘 필요가 없습니다. 다시 누르면 실행 중인 화면을 열고, 설정의 **Hycli 종료**에서 종료합니다. `hycli open`, `hycli status`, `hycli stop`도 사용할 수 있습니다. 아래 v0.1.0 다운로드와는 별도인 개발 중 패키지이며, OS별 검증·서명 상태는 [빌드 안내](../BUILD.md)와 [CI](https://github.com/Hybirdss/Hycli/actions/workflows/verify.yml)에서 확인하세요.

**[다운로드 v0.1.0 · Linux x64](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)** · glibc ≥ 2.39 · [SHA-256](https://github.com/Hybirdss/Hycli/releases/download/v0.1.0/hycli-0.1.0-linux-x64.tar.gz.sha256)

Hycli는 대시보드를 내장한 실행 파일 하나로 동작합니다. 위 링크에서 Linux x64 패키지와 `.sha256` 파일을 같은 폴더에 받은 뒤 실행하세요. 패키지를 사용하는 데 Rust나 Node.js는 필요하지 않습니다.

```sh
sha256sum -c hycli-0.1.0-linux-x64.tar.gz.sha256
tar -xzf hycli-0.1.0-linux-x64.tar.gz
cd hycli-0.1.0-linux-x64
./hycli dashboard
```

Linux x64·glibc 2.39 이상 환경에서 실행을 검증했습니다. 다른 Linux 환경과 macOS·Windows에서는 [빌드 준비 사항](../BUILD.md)을 확인하고 소스에서 빌드하세요.

<details>
<summary><strong>소스에서 빌드하기</strong></summary>

```sh
node scripts/build.mjs
./dist/hycli open
```

Windows에서는 `./dist/hycli.exe open`로 실행합니다.

</details>

대시보드는 `http://127.0.0.1:4318`에서 열립니다. 브라우저를 열지 않고 주소만 출력하려면 `hycli dashboard --no-open`을, 다른 포트를 사용하려면 `--port 4320`을 지정하세요. 웹사이트 엔진과 대시보드는 같은 실행 파일에 포함됩니다.

1. **AI 연결**을 열고 제공업체를 연결하세요.
2. 웹사이트 주소를 추가하고 준비 작업을 수행할 AI를 선택하세요.
3. 사용 가능한 작업을 검토하세요. 대시보드에서 실행하거나 **코딩 에이전트**를 열어 MCP 설정을 한 번 복사하세요.

로그인이 필요한 웹사이트는 **계정**에서 계정을 연결하세요. 웹사이트마다 여러 계정을 저장하고 도구에서 사용할 계정을 선택할 수 있습니다.

<a id="what-your-ai-can-do"></a>
## AI로 할 수 있는 일

Hycli는 지원되는 웹사이트 작업을 이름과 타입이 있는 도구로 변환합니다. AI가 작성한 설명에는 각 작업의 목적, 필요한 정보, 반환하는 결과가 담깁니다.

세 개의 독립적인 AI 작업자가 읽기와 검색, 유용한 작업 흐름, 계정 확인 자료를 각각 담당합니다. 대시보드에는 완료된 단계, 각 작업자의 상태, 경과 시간, 이해하기 쉬운 작업 메모가 표시됩니다. CLI와 MCP에서 수행한 작업도 같은 활동 기록에 나타납니다.

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="AI 작업자 셋, 그리고 아주 바쁜 작은 새 한 마리." width="100%" />
</p>
<p align="center"><sub>AI 작업자 셋, 그리고 아주 바쁜 작은 새 한 마리.</sub></p>

준비 과정은 웹사이트에서 실제로 얻을 수 있는 정보를 따릅니다. 연결된 문서, API 스키마, 공개된 JavaScript, 브라우저 도우미가 관찰한 요청 구조를 활용합니다. 작업을 확인하기 위해 범위가 제한된 읽기를 수행할 수 있습니다. 테스트 레코드를 만들거나 콘텐츠를 수정하거나 객체를 삭제하지 않으며, 대량의 엔드포인트를 추측하거나 실제 서버를 퍼징하지 않습니다.

| 작업 | 동작 |
| --- | --- |
| 읽기 또는 검색 | 근거가 있고 읽기 작업으로 분류되면 자율적으로 실행합니다. |
| 생성, 전송, 수정 또는 삭제 | 웹사이트, 계정, 작업, 확정된 입력값을 표시하고 사용자의 승인을 받습니다. |
| 영향이 불분명한 작업 | 요청을 보내기 전에 검토가 필요합니다. |
| 인증, 추가 확인 또는 요청 제한 | 해당 요청을 일시 중지하고 다시 연결하거나 기다릴 수 있도록 합니다. |

승인은 정확히 하나의 요청에 적용되며 5분 뒤 만료되고 한 번만 사용할 수 있습니다. 입력값, 계정, 저장된 로그인 정보, 설치된 도구 정의가 바뀌면 승인이 무효화됩니다. CLI, MCP, 대시보드 실행 모두 같은 경계를 적용합니다. 쓰기 작업은 자동으로 재시도하지 않습니다.

지원 범위는 웹사이트에 따라 다릅니다. 활용할 문서나 관찰된 작업이 없는 페이지에는 로그인된 브라우저, 별도로 제공한 SiteSpec, 추가 준비가 필요할 수 있습니다. Hycli는 모든 웹사이트에 바로 쓸 수 있는 API가 있다고 주장하지 않고 실제 지원 가능한 범위를 알려줍니다.

### 지원 범위

JSON·YAML OpenAPI, REST, GraphQL, JSON·URL 인코딩 폼 요청을 지원합니다. HTML에서는 관찰한 선택자로 목록·링크·다음 페이지 정보를 추출하며, 필요한 경우 로컬 Chromium으로 렌더링한 GET 페이지를 읽습니다. 준비 중 조회가 실패하면 AI가 근거를 확인해 정의를 수정하고 다시 검증합니다.

임의의 브라우저 클릭 흐름, multipart 업로드, 바이너리 다운로드는 현재 지원하지 않습니다. 계정과 기존 생성 CLI는 배포물에 포함되지 않습니다.

[작업 규격](../SITESPEC.md) · [배포 검증](../RELEASING.md)

<a id="ai-connections"></a>
## AI 연결

| 연결 | 로그인 |
| --- | --- |
| ChatGPT · API 키 | OpenAI API 키 |
| Claude | Anthropic API 키 |
| xAI | xAI API 키 |
| Z.ai | Z.ai API 키 |
| Z.ai Coding Plan | Z.ai Coding Plan API 키 |
| ChatGPT · 로그인 | 설치된 Codex app-server에서 관리하는 ChatGPT 로그인 |

대시보드에서 모델을 선택하세요. 해당 계정에서만 사용할 수 있는 모델도 선택할 수 있습니다. 연결 확인은 제공업체와 선택한 모델을 검증합니다. API 사용 요금은 제공업체 계정으로 청구됩니다.

Codex 연결은 공식 app-server 프로토콜을 사용합니다. Hycli는 Codex OAuth 토큰을 읽거나 복사하지 않습니다. 추론은 파일, 셸, 브라우저, 애플리케이션, MCP 실행이 비활성화된 임시 스레드에서 이루어집니다. 웹사이트 준비는 Hycli가 제어하는 읽기 도구가 수행합니다.

<a id="use-hycli-with-your-ai"></a>
## AI와 함께 Hycli 사용하기

**코딩 에이전트 → 연결 설정 보기**

일반 MCP 연결은 웹사이트 준비부터 실행까지 제공합니다. 에이전트를 한 번 연결하면 없는 도구를 준비하고, 진행 상황을 확인하고, 새 작업을 실행할 수 있습니다.

```sh
hycli mcp
```

설치된 작업만 공개하려면 `--sites-only`를 사용합니다. 한 웹사이트로 제한하는 연결은 다음과 같습니다.

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

`hycli`가 에이전트의 PATH에 없으면 대시보드에 표시된 실행 파일 경로를 사용하세요.

CLI에서도 같은 흐름을 사용할 수 있습니다. 예시 주소와 `SITE`, `ACTION`을 자신의 웹사이트와 `describe`가 반환한 이름으로 바꾸세요.

```sh
hycli prepare https://your-website.example --intent "저장한 참고 자료 찾기"
hycli request SITE "글 본문 편집, 초안 저장, 발행 기능을 추가해줘"
hycli describe
hycli describe SITE ACTION
hycli SITE ACTION --help
hycli run SITE ACTION --arg query="design systems"
hycli jobs show JOB_ID --watch
```

MCP에서는 `hycli_prepare`에 URL과 `intent`를 전달하고 `hycli_job` 또는 `hycli_result`로 완료까지 확인합니다. `hycli_run`은 클라이언트가 도구 목록을 새로 읽기 전에도 새 작업을 실행합니다. 기술적인 ID가 필요하면 관련 목록·검색 작업으로 먼저 찾습니다.

변경 작업은 대시보드 승인 내역을 반환합니다. 같은 요청을 반복하지 말고 기존 내역의 결과를 확인하세요. 조회 실패나 예상과 다른 응답은 CLI에서도 실패로 처리됩니다.

[에이전트 안내](../../agent/AGENTS.md) · [Hycli 스킬](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## 브라우저 로그인

웹사이트 준비는 현재 운영체제, 실행 중이거나 등록된 브라우저, 사용 가능한 프로필을 확인하는 것으로 시작합니다. AI는 해당 환경의 기능을 선택해 문서를 읽고, 페이지를 표시하고, 웹사이트 세션을 가져오고, 연결을 검증합니다. 브라우저와 프로필 경로는 로컬 컴퓨터에만 남으며, 생성된 웹사이트 정의는 개발자의 설치 경로에 의존하지 않습니다.

Chrome, Chromium, Edge, Brave, Firefox의 쿠키와 오리진 저장소는 접근할 수 있을 때 가져올 수 있습니다. 요청한 웹사이트의 세션만 Hycli의 로컬 보관소에 저장됩니다. Hycli는 검증된 신원 하나를 자동으로 선택합니다. 같은 신원에 속하는 프로필은 계정 선택 항목 하나로 취급합니다. 기존 계정 선택은 유지되며, 서로 다른 검증된 신원이 있으면 선택해야 합니다.

**계정 → 계정 연결**에서 **내 로그인 찾기**를 선택하면 다시 검색합니다. 사용할 수 있는 로그인 세션이 없으면 **웹사이트 로그인 페이지 열기**를 선택해 기본 브라우저에서 웹사이트를 여세요. 대화 상자가 열린 동안 Hycli가 다시 확인하며, 선택한 계정이 검증된 뒤에만 준비 작업을 이어갑니다. 탭을 열었다는 사실만으로 로그인이 완료된 것으로 표시하지 않습니다.

운영체제 보호가 적용되거나 브라우저에 묶인 세션, 비공개 세션, 컨테이너 세션은 브라우저 확장 프로그램이 필요할 수 있습니다. 확장 프로그램 탭을 선택해 연결 코드를 만든 다음, 로그인된 프로필에서 확장 프로그램을 열어 코드를 입력하고 해당 웹사이트에 대한 접근을 허용하세요. 기본 가져오기는 Windows App-Bound 보호를 우회하거나 브라우저 프로필의 보안을 약화하지 않습니다.

- **Chrome 및 Edge:** 패키지의 압축을 풀고 브라우저 확장 프로그램 페이지에서 **압축해제된 확장 프로그램을 로드합니다**를 사용하세요.
- **Firefox:** Firefox 패키지를 사용하고 `about:debugging`에서 **임시 부가 기능 로드**를 선택하세요. Firefox를 다시 시작하면 임시 확장 프로그램이 제거됩니다. 이번 릴리스에는 서명된 스토어 배포가 포함되지 않습니다.
- **쿠키 파일:** 대안으로 JSON 및 Netscape 형식의 쿠키 파일을 가져올 수 있습니다. 대시보드에서 파일을 직접 가져오세요. 내용을 AI 대화에 붙여넣지 마세요.

브라우저는 세션 데이터를 로컬 자격 증명 보관소에 직접 전달합니다. 모델에는 계정 라벨, 로컬 키 이름과 구조, 자격 증명 값이 제거된 도구 결과가 전달됩니다. 모델은 값을 받지 않고도 해당 로컬 값을 참조하는 연결 절차를 만들 수 있습니다. 쿠키의 적용 범위, 경로, 만료일, 지원되는 파티션 정보를 준수하며, 인증 헤더는 원래 오리진에서만 사용됩니다.

계정의 신원은 웹사이트에서 받은 실제 현재 계정 응답을 바탕으로 확인합니다. 브라우저 프로필 이름을 검증된 웹사이트 신원으로 취급하지 않습니다. 아직 계정을 식별하지 못했다면 Hycli가 이를 알립니다. 연결 후 평소처럼 탐색하면 준비 모델에 요청값, 헤더, 응답 본문을 노출하지 않고도 계정 응답과 사용 가능한 요청 구조를 제공할 수 있습니다.

웹사이트에 별도로 문서화된 API 자격 증명이나 로컬에서 사용할 수 없는 브라우저 기능이 필요할 수 있습니다. 준비 과정은 실제 연결 및 읽기 결과를 기록합니다. 홈페이지를 가져오거나 API에서 로그인 페이지가 반환되었다는 이유만으로 연동을 준비 완료로 표시하지 않습니다. [브라우저 설정 안내](../../browser-companion/guide.html)에서 확장 프로그램 절차와 한계를 설명합니다.

<a id="languages"></a>
## 언어

대시보드는 오른쪽에서 왼쪽으로 쓰는 아랍어를 포함해 위에 연결된 20개 언어를 지원합니다. 초기 언어는 영어입니다. 선택은 이 기기에 저장되며, 선택한 언어로 AI 설명을 준비할 수 있습니다.

<a id="local-data"></a>
## 로컬 데이터 및 요청 제한

웹사이트 정의, 계정 메타데이터, 활동 기록은 로컬 데이터 디렉터리에 보관됩니다. 다른 위치를 사용하려면 `HYCLI_DATA_DIR`을 설정하세요. **설정**에서 현재 위치와 자격 증명 보호 상태를 확인할 수 있습니다.

가능한 경우 자격 증명 보관소는 운영체제 키링으로 보호하는 암호화 키를 사용합니다. 키링을 사용할 수 없으면 파일 권한만으로 보호하는 저장소임을 명시하며, 비공개 디렉터리와 자격 증명 파일의 접근을 현재 운영체제 사용자로 제한합니다. 기존 암호화 보관소의 잠금을 풀 수 없다고 해서 알리지 않고 다른 저장소로 교체하지 않습니다.

요청은 웹사이트와 계정별로 순차 처리하고 속도를 조절하며, 웹사이트 전체에도 별도의 요청 한도를 적용합니다. Hycli는 `Retry-After`를 준수하고 응답 크기와 읽기 재시도 횟수를 제한하며, 인증 실패나 사이트의 추가 확인이 발생하면 일시 중지합니다. 웹사이트 준비 과정에서는 제한을 피하려고 계정을 바꾸거나 지문을 위조하거나 추가 확인을 우회하지 않습니다. 이런 조치는 불필요한 부하를 줄이지만 웹사이트가 계정을 절대 제한하지 않는다고 보장할 수는 없습니다.

사설 및 로컬 웹사이트 주소는 기본적으로 비활성화됩니다. 신뢰하는 자체 호스팅 사이트를 사용하려면 대시보드나 MCP 서버 실행 시 `--allow-local`을 명시하세요.

<a id="contributing"></a>
## 개발 및 검증

```sh
npm --prefix dashboard ci
npm --prefix dashboard run check
npm --prefix dashboard run build
cargo test --all-targets --locked
cargo build --locked --bin hycli --example dashboard_fixture
node scripts/test-e2e.mjs --full
```

테스트는 합성된 로컬 웹사이트와 제공업체 응답을 사용합니다. 승인과 요청의 결합 및 재사용 방지, 비밀 정보 제거, 리디렉션, 요청 속도 제한, 쓰기 재시도 금지, 브라우저 세션 보호, 제공업체 응답 계약을 검증합니다. Hycli를 테스트할 목적으로 실제 계정에서 콘텐츠를 만들거나 수정하거나 삭제하지 마세요.

스크린샷과 GIF는 격리된 예시 데이터를 사용합니다. [시각 자료 출처](../images/README.md)를 참고하세요.

## 라이선스 및 사용 목적

Hycli는 [Apache-2.0](../../LICENSE) 라이선스로 배포됩니다.

Hycli는 웹사이트를 더 편리하게 사용할 수 있는 명령줄 도구를 만들고 사용하기 위한 소프트웨어입니다. 접근 권한이 있는 웹사이트와 계정에 사용하세요.

소프트웨어는 보증 없이 있는 그대로 제공됩니다. 법이 허용하는 범위에서 저작자와 기여자는 사용으로 인한 손실이나 기타 문제에 책임을 지지 않습니다. 사용 방식에 대한 책임은 사용자에게 있습니다.

법이 허용하는 범위에서 저작자와 기여자는 Hycli 사용으로 발생하는 계정 차단·정지·이용 제한에 책임을 지지 않습니다. 해킹, 무단 접근 또는 공격 목적으로 사용하지 마세요.
