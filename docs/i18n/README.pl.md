<p align="center">
  <img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="Hycli — Strony internetowe gotowe do pracy z AI." width="100%" />
</p>

<p align="center">
  <strong>Zamień dowolną stronę internetową w typowane narzędzie CLI i serwer MCP dla swojego agenta AI.</strong><br/>
  <sub>Czytaj, wyszukuj, twórz, edytuj i automatyzuj — korzystając z prawdziwego logowania w przeglądarce, bez wycieków plików cookie do modeli LLM i z wbudowaną odpornością na blokady antybotowe.</sub>
</p>

<p align="center">
  <a href="https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0">Pobierz (Windows · macOS · Linux)</a> ·
  <a href="https://github.com/Hybirdss/Hycli/issues">Uzyskaj pomoc na GitHubie</a> ·
  <a href="#try-it-in-30-seconds">Wypróbuj w 30 sekund</a> ·
  <a href="#get-started">Pierwsze kroki</a> ·
  <a href="#what-your-ai-can-do">Co może robić Twoje AI</a> ·
  <a href="#common-concerns">Częste obawy</a> ·
  <a href="#use-hycli-with-your-ai">Używaj z AI</a>
</p>

<details>
<summary>Czytaj w swoim języku · 20 języków</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

Podaj Hycli dowolny adres URL strony internetowej i opisz swój cel. Trzech workerów AI analizuje dokumentację, paczki JS frontendu oraz rzeczywisty ruch sieciowy przeglądarki, aby zbudować typowany zestaw narzędzi CLI i MCP — obejmujący zarówno odczyt, jak i zapis — nie wykonując żadnych zmian na Twoim koncie podczas przygotowania.

<p align="center">
  <img src="../../docs/images/dashboard.png" alt="Panel Hycli z kartami stron, dostępnymi działaniami i postępem zadań." width="100%" />
</p>
<p align="center"><sub>Jeden lokalny panel i CLI dla Twoich stron internetowych, kont, uprawnień zapisu i agentów AI.</sub></p>

<a id="try-it-in-30-seconds"></a>
## Wypróbuj w 30 sekund

```sh
# 1. Wskaż Hycli dowolną stronę i opisz cel (przygotowuje odczyt i zapis; zero zmian na żywo)
hycli prepare https://your-website.example --intent "Wyszukuj wpisy, zapisuj szkice i publikuj artykuły"

# 2. Zobacz wszystkie przygotowane operacje odczytu, zapisu i wieloetapowe przepływy pracy
hycli describe your-website

# 3. Uruchamiaj operacje odczytu natychmiast
hycli your-website search-posts --query "informacje o wydaniu"

# 4. Włącz zapis jednorazowo dla tego CLI, gdy zechcesz tworzyć, edytować lub usuwać
hycli writes your-website allow
hycli your-website create-draft --title "Cześć z Hycli" --body "Zautomatyzowane prosto z terminala."
```

Lub połącz Hycli z **Claude Code, Cursor, Codex albo Windsurf** raz za pomocą `"command": "hycli", "args": ["mcp"]` i poproś agenta w naturalnym języku:
> *"Użyj Hycli, aby przygotować `https://...`, pokaż mi dostępne operacje i utwórz szkic nowego wpisu."*

<a id="get-started"></a>
## Pierwsze kroki

Hycli jest dostarczany jako pojedynczy natywny plik binarny zawierający zarówno silnik obsługi stron, jak i lokalny panel (`http://127.0.0.1:4318`). Do uruchomienia gotowego pakietu nie są potrzebne ani Rust, ani Node.js.

### Pobieranie i uruchamianie (Windows, macOS, Linux)

Pobierz natywny instalator lub archiwum przenośne dla swojego systemu z **[GitHub Releases (v0.1.0)](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)**:

| System | Architektura | Natywny instalator | Archiwum przenośne | Jak uruchomić |
| --- | --- | --- | --- | --- |
| **Windows 10/11** | x64 i ARM64 | `hycli-0.1.0-win32-*-setup.exe` | `hycli-0.1.0-win32-*.tar.gz` | Uruchom `*-setup.exe` (dodaje `hycli` do zmiennej `PATH` użytkownika i menu Start), a następnie otwórz **Hycli** lub wpisz `hycli open` w PowerShell. |
| **macOS 13+** | Apple Silicon (`arm64`) i Intel (`x64`) | `hycli-0.1.0-darwin-*.dmg` | `hycli-0.1.0-darwin-*.tar.gz` | Otwórz plik `.dmg`, przeciągnij **Hycli.app** do **Applications** i uruchom aplikację (automatycznie dowiązuje `hycli` w `~/.local/bin`). |
| **Linux** | x64 i ARM64 | `hycli-0.1.0-linux-*.deb` | `hycli-0.1.0-linux-*.tar.gz` | Zainstaluj pakiet `.deb` (`sudo apt install ./hycli-*.deb`) lub rozpakuj `.tar.gz` i uruchom `./install.sh`, a następnie wpisz `hycli open`. |

<details>
<summary><strong>Szybka instalacja z terminala i budowanie ze źródeł</strong></summary>

**Archiwum przenośne Linux / macOS:**

```sh
sha256sum -c hycli-0.1.0-*.tar.gz.sha256   # na macOS: shasum -a 256 -c
tar -xzf hycli-0.1.0-*.tar.gz
cd hycli-0.1.0-*
./install.sh
hycli open
```

**Archiwum przenośne Windows PowerShell:**

```powershell
tar -xzf hycli-0.1.0-win32-x64.tar.gz
cd hycli-0.1.0-win32-x64
powershell -ExecutionPolicy Bypass -File .\install.ps1
hycli open
```

**Budowanie ze źródeł (Linux, macOS, Windows):**

Zainstaluj [wymagane narzędzia do budowania](../BUILD.md) i uruchom:

```sh
node scripts/build.mjs
./dist/hycli open
```

Uruchom `node scripts/package-desktop.mjs`, aby utworzyć natywny instalator `.exe`, `.dmg` lub `.deb` dla swojego systemu. Zobacz [instrukcję budowania i pakowania](../BUILD.md) oraz [weryfikację wydań](../RELEASING.md).

</details>

### Podłącz pierwszą stronę internetową

1. Uruchom `hycli open` (lub kliknij ikonę aplikacji **Hycli**), aby otworzyć lokalny panel pod adresem `http://127.0.0.1:4318`.
2. Otwórz **Połączenia z AI** i podłącz OpenAI, Anthropic Claude, xAI, Z.ai lub lokalne logowanie ChatGPT (Codex).
3. Dla stron wymagających logowania kliknij **Konta → Znajdź moje logowanie**, aby zaimportować sesję przeglądarki do lokalnego sejfu.
4. Wpisz adres URL strony i opcjonalny cel albo skopiuj konfigurację MCP z sekcji **Agenci programistyczni** do swojego asystenta AI.

<a id="what-your-ai-can-do"></a>
## Co może robić Twoje AI

Hycli zamienia rzeczywiste interfejsy stron internetowych w samodokumentujące się podpolecenia CLI i narzędzia MCP:

| Interfejs strony | Przygotowana funkcjonalność |
| --- | --- |
| REST i OpenAPI | Typowane operacje odczytu i zapisu oparte na schematach JSON/YAML i dokumentacji API. |
| GraphQL | Sparsowane dokumenty zapytań (`query`) i mutacji (`mutation`) z typowanymi zmiennymi JSON. |
| Strony HTML i renderowane przez JS | Ustrukturyzowane rekordy, linki, stronicowanie oraz odczyty stron GET renderowanych przez lokalny silnik Chromium. |
| Formularze JSON i kodowane w URL | Udokumentowane pola wejściowe, wartości domyślne i wysyłanie formularzy w kontekście konta (`POST`, `PUT`, `PATCH`, `DELETE`). |

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="Trzech workerów AI i jeden bardzo zapracowany mały ptaszek." width="100%" />
</p>
<p align="center"><sub>Trzech workerów AI i jeden bardzo zapracowany mały ptaszek.</sub></p>

| Typ działania | Sposób wykonania |
| --- | --- |
| **Odczyt lub wyszukiwanie** | Uruchamiane natychmiast, gdy ma potwierdzenie w dowodach i zostało zaklasyfikowane jako bezpieczny odczyt. |
| **Tworzenie, wysyłanie, edycja lub usuwanie** | Wykrywane i dodawane do specyfikacji CLI bez wykonywania podczas przygotowania. Domyślnie wyłączone (`writes: false`). Agent AI pyta Cię **raz dla danego CLI**, czy włączyć zapis (`hycli writes SITE allow` lub MCP `hycli_writes`). Po włączeniu operacje zapisu wykonują się bezpośrednio na Twoje polecenie. |
| **Niejasny skutek** | Wstrzymywane do przeglądu przed wysłaniem. |
| **Wyzwanie uwierzytelniania lub limit żądań** | Przełącza strategię transportu (HTTP → `window.fetch` w kontekście strony → ukryty Chromium) lub bezpiecznie wstrzymuje działanie. |

Zobacz [specyfikację operacji](../SITESPEC.md), aby poznać obsługiwane formaty żądań i reguły ekstrakcji.

<a id="common-concerns"></a>
## Częste obawy i jak Hycli je rozwiązuje

Łączenie agenta AI z prawdziwymi stronami internetowymi i zalogowanymi kontami rodzi ważne pytania. Oto jak Hycli rozwiązuje każde z nich już na poziomie architektury:

### 1. „Czy AI przypadkowo opublikuje, zmieni lub usunie dane na moim koncie?"
- **Zero zmian na żywo podczas tworzenia CLI:** Podczas przygotowywania lub sprawdzania strony (`hycli prepare`, `hycli request`, `hycli recon`, `hycli check`) Hycli wykrywa i kompiluje w specyfikacji CLI zarówno operacje odczytu, jak i zapisu, ale **nigdy nie wykonuje żądań zapisu**. Nawet ukryte mutacje (ścieżki `GET` zmieniające stan, takie jak `/delete`, zapytania GraphQL `mutation` czy formularze HTML `<form method="post">`) są blokowane podczas weryfikacji.
- **Jednorazowe potwierdzenie zapisu dla każdego CLI:** Każde nowo utworzone CLI strony zaczyna z ustawieniem `writes: false`. Gdy agent AI po raz pierwszy potrzebuje wykonać operację zapisu w danym CLI, pyta Cię **jeden raz dla tego CLI**: *„Zezwolić na operacje zapisu dla `<site>`?"* Dopiero po Twojej zgodzie uruchamia `hycli writes <site> allow` (lub MCP `hycli_writes`), odblokowując bezpośrednie wykonywanie zapisu. Uprawnienie możesz w każdej chwili cofnąć poleceniem `hycli writes <site> deny` lub przełącznikiem **Zezwalaj na zmiany** w panelu.
- **Brak automatycznych ponowień zapisu:** Nieudane lub przerwane żądania zapisu nigdy nie są ponawiane automatycznie. Jeśli połączenie zostanie zerwane po wysłaniu mutacji, Hycli zwraca `action_outcome_unknown`, abyś mógł sprawdzić stan na stronie zamiast duplikować zamówienie lub wpis.

### 2. „Czy Hycli wysyła moje pliki cookie, tokeny logowania lub klucze API do modelu AI?"
- **Sekrety nigdy nie trafiają do promptów LLM:** Pliki cookie przeglądarki, tokeny `localStorage` / `sessionStorage` oraz klucze API są przechowywane wyłącznie w lokalnym sejfie (zaszyfrowanym przez systemowy pęk kluczy: Menedżer poświadczeń systemu Windows, pęk kluczy macOS, Linux Secret Service lub chronionym uprawnieniami `0600` tylko dla właściciela).
- **Symboliczne przepisy połączenia (`ConnectionRecipe`):** Modele AI widzą jedynie etykiety kont, nazwy kluczy pamięci i zredagowane schematy odpowiedzi. Model tworzy symboliczny `ConnectionRecipe` (odwołujący się tylko do nazwy pliku cookie lub tokenu), a Hycli wstrzykuje właściwy sekret lokalnie w momencie wykonania i usuwa sekrety ze wszystkich wyników oraz logów.
- **Import tylko do odczytu ograniczony do domeny:** Importowanie sesji przeglądarki odczytuje wyłącznie pliki cookie i pamięć docelowej domeny z tymczasowej migawki tylko do odczytu. Nigdy nie osłabia zabezpieczeń przeglądarki ani nie eksportuje danych innych stron.

### 3. „Czy pobieranie danych wywoła CAPTCHA lub doprowadzi do zablokowania konta?"
- **Brak fuzzingu i skanowania siłowego:** Hycli nigdy nie zgaduje list adresów URL ani nie przeprowadza fuzzingu na działających serwerach. Uczy się wyłącznie z podlinkowanej dokumentacji, schematów OpenAPI/GraphQL, statycznych paczek JS i zaobserwowanego ruchu sieciowego przeglądarki.
- **Szeregowy ruch z kontrolą tempa:** Żądania są wykonywane szeregowo i mają ograniczoną częstotliwość dla każdej strony i każdego konta, z poszanowaniem nagłówka `Retry-After` oraz limitami ponowień odczytu.
- **Wyłącznik bezpieczeństwa strategii i wykonanie w natywnej przeglądarce:** Gdy zwykłe żądanie HTTP napotka błąd 403/429 lub wyzwanie antybotowe (Cloudflare, Akamai Bot Manager), Hycli nie zapętla zablokowanego żądania. Wyłącznik bezpieczeństwa natychmiast przełącza transport — wykonując `window.fetch(..., { credentials: 'include' })` wewnątrz uwierzytelnionego kontekstu przeglądarki lub korzystając z lokalnego silnika ukrytego Chromium — albo bezpiecznie wstrzymuje działanie, aby chronić Twoje konto.

### 4. „Czy przekierowanie może ujawnić moją sesję, a udostępniona specyfikacja zdradzić ścieżki na moim komputerze?"
- **Ścisłe przypisanie do domeny pochodzenia (Origin):** Nagłówki uwierzytelniania i pliki cookie są przypisane do docelowego originu i natychmiast usuwane przy przekierowaniach między domenami. Prywatne i lokalne zakresy adresów IP są domyślnie blokowane, chyba że jawnie podano flagę `--allow-local`.
- **Przenośne specyfikacje:** Ścieżki do plików wykonywalnych lokalnych przeglądarek i profili pozostają w stanie lokalnym maszyny i nigdy nie są zapisywane w przenośnych plikach `SiteSpec`.

### 5. „Czy AI będzie marnować tokeny na ponawianie prób, gdy moje logowanie wygaśnie?"
- **Deterministyczna kontrola stanu bez AI (`hycli check`):** Polecenie `hycli check` (oraz MCP `hycli_check`) odtwarza zweryfikowane łańcuchy odczytu bez wywoływania modelu LLM i precyzyjnie odróżnia `signed_out` (połącz konto ponownie) od `site_changed` (napraw specyfikację), `blocked` oraz `unreachable`.

<a id="ai-connections"></a>
## Połączenia z AI

| Dostawca | Uwierzytelnianie |
| --- | --- |
| ChatGPT · klucz API | Klucz API OpenAI |
| Claude | Klucz API Anthropic |
| xAI | Klucz API xAI |
| Z.ai / Z.ai Coding Plan | Klucz API Z.ai |
| ChatGPT · logowanie | Lokalne logowanie ChatGPT przez oficjalny protokół Codex app-server |

Połączenie logowania Codex działa w tymczasowym wątku z wyłączonym dostępem do plików, powłoki, przeglądarki i zewnętrznych serwerów MCP; Hycli nigdy nie odczytuje ani nie kopiuje tokenów OAuth narzędzia Codex.

<a id="use-hycli-with-your-ai"></a>
## Używaj Hycli ze swoim AI

Dodaj Hycli do klienta MCP (**Agenci programistyczni → Zobacz ustawienia połączenia** pokazuje dokładną ścieżkę dla Twojego systemu):

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

Użyj `hycli mcp --sites-only`, aby udostępnić tylko zainstalowane narzędzia stron, lub dodaj `--only-site SITE`, aby ograniczyć dostęp do jednej strony. Serwer MCP udostępnia `hycli_prepare`, `hycli_writes` (włącza zapis raz dla danego CLI po uzyskaniu Twojej zgody), `hycli_run`, `hycli_check`, `hycli_result` oraz `hycli_activity` — i nigdy nie udostępnia narzędzia do odczytu danych uwierzytelniających.

Z poziomu terminala każdy przepływ pracy działa dokładnie tak samo:

```sh
hycli prepare https://your-website.example --intent "Znajdź zapisane materiały"
hycli request SITE "Dodaj edycję szkiców i publikowanie z pełną treścią wpisu"
hycli describe SITE
hycli SITE ACTION --help
hycli writes SITE allow
hycli SITE ACTION --query "design systems"
hycli check SITE
```

 [Przewodnik dla agentów](../../agent/AGENTS.md) · [Skill Hycli](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## Logowanie przez przeglądarkę

Hycli wykrywa lokalne profile przeglądarek Chrome, Chromium, Edge, Brave, Vivaldi, Arc, Whale i Firefox (oraz zainstalowane lokalnie binaria ukrytego Chromium) w systemach Windows, macOS i Linux, importując do lokalnego sejfu wyłącznie sesję żądanej strony. Profile współdzielące to samo zweryfikowane konto na stronie są automatycznie łączone.

Jeśli sesja jest zablokowana przez szyfrowanie Windows App-Bound, prywatne kontenery lub niezaimportowaną przeglądarkę, użyj **Konta → Połącz konto**, aby zalogować się przez przeglądarkę, połącz [rozszerzenie towarzyszące przeglądarki](../../browser-companion/guide.html) albo zaimportuj plik cookie JSON/Netscape w panelu (nigdy nie wklejaj plików cookie do czatu z AI).

<a id="languages"></a>
## Języki

Panel oraz generowane przez AI opisy działań obsługują 20 języków podlinkowanych na górze tego pliku README, w tym język arabski (RTL).

<a id="local-data"></a>
## Dane lokalne i limity żądań

Definicje stron, zaszyfrowane dane uwierzytelniające i historia aktywności znajdują się w katalogu danych systemu operacyjnego (`%LOCALAPPDATA%\hycli` w Windows, `~/Library/Application Support/hycli` w macOS, `~/.local/share/hycli` w Linux lub `HYCLI_DATA_DIR`). Stan ochrony sejfu możesz w każdej chwili sprawdzić w **Ustawieniach**.

<a id="contributing"></a>
## Rozwój i weryfikacja

```sh
npm --prefix dashboard ci
npm --prefix dashboard run check
npm --prefix dashboard run build
cargo test --all-targets --locked
cargo build --locked --bin hycli --example dashboard_fixture
node scripts/test-e2e.mjs --full
```

Wszystkie testy są uruchamiane na syntetycznych lokalnych stronach i zestawach danych testowych — nigdy nie testuj operacji zapisu na prawdziwym koncie w zewnętrznym serwisie. Zobacz [źródła materiałów graficznych](../images/README.md).

## Licencja i przeznaczenie

Hycli jest udostępniany na licencji [Apache-2.0](../../LICENSE). Używaj go wyłącznie ze stronami i kontami, do których masz uprawniony dostęp. Oprogramowanie jest dostarczane w stanie „takim, w jakim jest" (as is), bez żadnych gwarancji; autorzy i współtwórcy nie ponoszą odpowiedzialności za jakiekolwiek straty, ograniczenia kont ani blokady wynikające z jego używania.

## Star History

<a href="https://www.star-history.com/#Hybirdss/Hycli&Date">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date&theme=dark" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
 </picture>
</a>
