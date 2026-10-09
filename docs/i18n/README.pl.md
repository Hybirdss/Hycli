<p align="center">
<img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="Hycli — Strony gotowe do pracy z AI." width="100%" />
</p>

<p align="center"><strong>Zamień strony internetowe w narzędzia dla swojego AI.</strong></p>

<p align="center">
<a href="#get-started">Pierwsze kroki</a> · <a href="#what-your-ai-can-do">Co może robić Twoje AI</a> · <a href="#ai-connections">Połączenia z AI</a> · <a href="#use-hycli-with-your-ai">Używaj z AI</a>
</p>

<details>
<summary>Czytaj w swoim języku · 20 języków</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

Dodaj stronę. Hycli przygotuje działania, z których może korzystać Twoje AI, i wyjaśni, do czego służy każde z nich. Strony, dane logowania i wyniki znajdziesz razem w lokalnym panelu.

| Połącz | Przygotuj | Korzystaj |
| --- | --- | --- |
| Dodaj stronę i wybierz AI. | Śledź postępy i sprawdź dostępne działania. | Uruchom działanie lub udostępnij je swojemu asystentowi AI. |

<p align="center"><img src="../../docs/images/dashboard.png" alt="Panel Hycli z kartami stron, dostępnymi działaniami i postępem zadań." width="100%" /></p>
<p align="center"><sub>Przykładowa przestrzeń robocza z przykładowymi narzędziami i kontami.</sub></p>

<a id="get-started"></a>
## Pierwsze kroki

Nowe pakiety aplikacji: Windows (`*-setup.exe`), macOS (`.dmg`) i Linux (`.deb` lub `./install.sh` po rozpakowaniu). Otwórz Hycli za pomocą ikony: silnik uruchomi się w tle i otworzy przeglądarkę. Terminal nie musi pozostać otwarty. Zamknij aplikację w ustawieniach lub użyj `hycli open`, `hycli status` i `hycli stop`. Te pakiety rozwojowe są oddzielne od v0.1.0; sprawdź weryfikację platform i podpisy w [instrukcji](../BUILD.md) oraz [CI](https://github.com/Hybirdss/Hycli/actions/workflows/verify.yml).

**[Pobierz v0.1.0 · Linux x64](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)** · glibc ≥ 2.39 · [SHA-256](https://github.com/Hybirdss/Hycli/releases/download/v0.1.0/hycli-0.1.0-linux-x64.tar.gz.sha256)

Hycli to aplikacja napisana w Rust z lokalnym panelem w przeglądarce. W tej kopii repozytorium zbuduj panel i plik wykonywalny:

```sh
node scripts/build.mjs
./dist/hycli open
```

Panel otworzy się pod adresem `http://127.0.0.1:4318`. Polecenie `hycli dashboard --no-open` wyświetla adres bez otwierania przeglądarki. Opcja `--port 4320` pozwala wybrać inny port. Mechanizm obsługi stron i panel są zawarte w jednym pliku wykonywalnym.

1. Otwórz **Połączenia z AI** i połącz dostawcę.
2. Dodaj adres strony i wybierz AI, które ją przygotuje.
3. Przejrzyj dostępne działania. Uruchom jedno w panelu lub otwórz **Agenci programistyczni**, aby raz skopiować konfigurację MCP.

Jeśli strona wymaga logowania, połącz konto w sekcji **Konta**. Dla jednej strony możesz zapisać kilka kont i wybrać, którego używają jej narzędzia.

<a id="what-your-ai-can-do"></a>
## Co może robić Twoje AI

Hycli zamienia obsługiwane operacje strony w narzędzia z nazwami i określonymi typami danych. Opisy tworzone przez AI wyjaśniają, co robi każde działanie, jakich danych potrzebuje i co zwraca.

Trzej niezależni agenci AI zajmują się odczytem i wyszukiwaniem, przydatnymi przepływami pracy oraz informacjami o koncie. Panel pokazuje ukończone etapy, stan każdego agenta, czas trwania i zrozumiałe notatki z pracy. Działania CLI i MCP trafiają do tej samej historii aktywności.

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="Trzech workerów. Jeden bardzo zapracowany ptaszek." width="100%" />
</p>
<p align="center"><sub>Trzech workerów. Jeden bardzo zapracowany ptaszek.</sub></p>

Przygotowanie opiera się na informacjach faktycznie dostępnych na stronie: dokumentacji, do której prowadzą odnośniki, schematach API, opublikowanym kodzie JavaScript i strukturach żądań zaobserwowanych przez dodatek do przeglądarki. Aby sprawdzić operację, Hycli może wykonywać odczyty w ograniczonym zakresie. Nie tworzy rekordów testowych, nie edytuje treści, nie usuwa obiektów, nie zgaduje długich list adresów API i nie wysyła losowych danych testowych do działającego serwera.

| Działanie | Zachowanie |
| --- | --- |
| Odczyt lub wyszukiwanie | Wykonywane samodzielnie, jeśli operacja ma potwierdzenie w dostępnych danych i została zaklasyfikowana jako odczyt. |
| Tworzenie, wysyłanie, edycja lub usuwanie | Wyświetla stronę, konto, działanie i ustalone dane wejściowe do zatwierdzenia przez użytkownika. |
| Niejasny skutek | Wymaga sprawdzenia przed wysłaniem żądania. |
| Uwierzytelnianie, dodatkowa weryfikacja lub limit żądań | Wstrzymuje odpowiednie żądania i pozwala ponownie się połączyć lub poczekać. |

Zgoda dotyczy jednego konkretnego żądania, wygasa po pięciu minutach i może zostać wykorzystana tylko raz. Zmiana danych wejściowych, konta, zapisanych danych logowania lub zainstalowanej definicji narzędzia ją unieważnia. Ta sama zasada obowiązuje w CLI, MCP i panelu. Operacje zapisu nigdy nie są automatycznie ponawiane.

Zakres obsługi zależy od strony. Strona bez użytecznej dokumentacji lub zaobserwowanych operacji może wymagać przeglądarki z zalogowanym kontem, dostarczenia SiteSpec albo dalszego przygotowania. Hycli informuje, co może obsłużyć, zamiast zakładać, że każda strona ma gotowe API.

### Obsługiwane możliwości

Obsługiwane są OpenAPI w JSON lub YAML, REST, GraphQL oraz żądania JSON i formularze kodowane dla adresów URL. Dane, linki i informacje o następnej stronie można pobierać z HTML za pomocą zaobserwowanych selektorów; dostępne są też odczyty stron GET renderowanych lokalnym Chromium. Jeśli odczyt podczas przygotowania zawiedzie, AI analizuje dowody, poprawia definicję i sprawdza ją ponownie.

Dowolne sekwencje kliknięć, przesyłanie multipart i pobieranie plików binarnych nie są jeszcze obsługiwane. Pakiety nie zawierają kont ani wcześniej wygenerowanych CLI.

[Format działań](../SITESPEC.md) · [Weryfikacja pakietów](../RELEASING.md)

<a id="ai-connections"></a>
## Połączenia z AI

| Połączenie | Logowanie |
| --- | --- |
| ChatGPT · API key | Twój klucz API OpenAI |
| Claude | Twój klucz API Anthropic |
| xAI | Twój klucz API xAI |
| Z.ai | Twój klucz API Z.ai |
| Z.ai Coding Plan | Twój klucz API Z.ai Coding Plan |
| ChatGPT · login | Logowanie przez ChatGPT obsługiwane przez zainstalowany Codex app-server |

Wybierz model w panelu, także taki, który jest dostępny specjalnie dla Twojego konta. Sprawdzenie połączenia weryfikuje dostawcę i wybrany model. Dostawcy API rozliczają użycie na Twoim koncie u dostawcy.

Połączenie z Codex korzysta z jego oficjalnego protokołu app-server. Hycli nie odczytuje ani nie kopiuje tokenów OAuth Codex. Model pracuje w tymczasowym wątku z wyłączoną możliwością wykonywania operacji przez pliki, powłokę, przeglądarkę, aplikacje i MCP. Strony są przygotowywane za pomocą kontrolowanych narzędzi odczytu Hycli.

<a id="use-hycli-with-your-ai"></a>
## Używaj Hycli ze swoim AI

**Agenci programistyczni → Zobacz ustawienia połączenia**

Ogólne połączenie MCP umożliwia przygotowanie witryn i wykonywanie działań. Po jednorazowym połączeniu agent może przygotować brakujące narzędzia, śledzić postęp i używać nowych działań.

```sh
hycli mcp
```

Użyj `--sites-only`, aby udostępniać tylko zainstalowane działania. Poniższe połączenie jest ograniczone do jednej witryny.

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

Jeśli `hycli` nie znajduje się w PATH agenta, użyj ścieżki programu wyświetlanej w panelu.

Ten sam przebieg jest dostępny w CLI. Zastąp przykładowy adres, `SITE` i `ACTION` własną witryną oraz nazwami zwróconymi przez `describe`.

```sh
hycli prepare https://your-website.example --intent "Znajdź zapisane materiały"
hycli request SITE "Add draft editing and publishing with full post content"
hycli describe
hycli describe SITE ACTION
hycli SITE ACTION --help
hycli run SITE ACTION --arg query="design systems"
hycli jobs show JOB_ID --watch
```

W MCP przekaż adres i `intent` do `hycli_prepare`, a następnie śledź zadanie przez `hycli_job` lub `hycli_result`. `hycli_run` wykonuje nowe działania jeszcze przed odświeżeniem listy narzędzi przez klienta. Wewnętrzne identyfikatory wyszukuj najpierw przez powiązane działania listowania lub wyszukiwania.

Zmiany zwracają potwierdzenie do sprawdzenia w panelu. Śledź wynik bez ponawiania żądania. Nieudany odczyt lub nieoczekiwana odpowiedź oznacza również błąd CLI.

[Przewodnik dla agentów](../../agent/AGENTS.md) · [Skill Hycli](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## Logowanie przez przeglądarkę

Przygotowanie witryny zaczyna się od sprawdzenia bieżącego systemu operacyjnego, uruchomionych i zarejestrowanych przeglądarek oraz dostępnych profili. AI wybiera spośród tych możliwości, aby czytać dokumentację, wyświetlać strony, importować sesję witryny i sprawdzać połączenie. Ścieżki przeglądarek i profili pozostają na lokalnym komputerze; wygenerowana definicja witryny nie zależy od ścieżek instalacyjnych dewelopera.

Pliki cookie i dane przechowywane dla danej witryny z Chrome, Chromium, Edge, Brave i Firefox można importować, gdy są dostępne. Do lokalnego sejfu Hycli trafia tylko sesja żądanej witryny. Hycli automatycznie wybiera jedną zweryfikowaną tożsamość; profile należące do tej samej tożsamości liczą się jako jedna opcja konta. Istniejący wybór konta zostaje zachowany; różne zweryfikowane tożsamości wymagają dokonania wyboru.

W sekcji **Konta → Połącz konto** opcja **Znajdź moje logowanie** ponawia wyszukiwanie. Jeśli nie znajdzie odpowiedniej sesji, **Otwórz stronę logowania** otworzy stronę w domyślnej przeglądarce. Hycli sprawdza ponownie, dopóki okno dialogowe jest otwarte, i wznawia przygotowanie dopiero po weryfikacji wybranego konta. Samo otwarcie karty nie oznacza pomyślnego logowania.

Sesje chronione przez system operacyjny, powiązane z przeglądarką, prywatne lub kontenerowe mogą wymagać dodatku do przeglądarki. Wybierz kartę dodatku i utwórz kod połączenia, a następnie otwórz dodatek w zalogowanym profilu, wpisz kod i przyznaj dostęp do tej strony. Bezpośredni import nie omija ochrony Windows App-Bound ani nie osłabia profilu przeglądarki.

- **Chrome i Edge:** rozpakuj paczkę i wybierz **Załaduj rozpakowane** na stronie rozszerzeń przeglądarki.
- **Firefox:** użyj paczki dla Firefox i opcji **Wczytaj tymczasowy dodatek** w `about:debugging`. Tymczasowe rozszerzenia są usuwane po ponownym uruchomieniu Firefox. Ta wersja nie obejmuje dystrybucji podpisanego dodatku przez sklep.
- **Plik cookie:** awaryjnie można importować pliki cookie w formacie JSON lub Netscape. Importuj plik bezpośrednio w panelu; nie wklejaj jego zawartości do rozmowy z AI.

Przeglądarka przesyła dane sesji bezpośrednio do lokalnego sejfu danych uwierzytelniających. Modele otrzymują etykiety kont, nazwy i strukturę lokalnych kluczy oraz wyniki narzędzi bez wartości danych uwierzytelniających. Mogą utworzyć przepis połączenia odwołujący się do tych lokalnych wartości, nie otrzymując ich. Przestrzegane są zakres, ścieżki, terminy wygaśnięcia i obsługiwane informacje o partycjonowaniu plików cookie; nagłówki uwierzytelniania pozostają ograniczone do pierwotnej domeny pochodzenia.

Tożsamość konta jest ustalana na podstawie rzeczywistej odpowiedzi strony dotyczącej bieżącego konta. Nazwa profilu przeglądarki nie jest uznawana za potwierdzoną tożsamość na stronie. Jeśli konto nie zostało jeszcze rozpoznane, Hycli to zgłasza. Po połączeniu zwykłe przeglądanie może dostarczyć odpowiedź z danymi konta i struktury dostępnych żądań bez ujawniania modelowi przygotowującemu stronę wartości żądań, nagłówków ani treści odpowiedzi.

Witryna może wymagać osobnych, udokumentowanych danych uwierzytelniających API lub funkcji przeglądarki niedostępnej lokalnie. Przygotowanie zapisuje rzeczywiste wyniki połączenia i odczytu. Pobranie strony głównej lub otrzymanie strony logowania z API nie oznacza, że integracja jest gotowa. [Przewodnik konfiguracji przeglądarki](../../browser-companion/guide.html) opisuje działanie rozszerzenia i jego ograniczenia.

<a id="languages"></a>
## Języki

Panel obsługuje 20 języków wymienionych powyżej, w tym arabski z zapisem od prawej do lewej. Początkowym językiem jest angielski. Wybór zostaje zapisany na tym urządzeniu, a opisy AI można przygotować w wybranym języku.

<a id="local-data"></a>
## Dane lokalne i limity żądań

Definicje stron, metadane kont i aktywność pozostają w lokalnym katalogu danych. Ustaw `HYCLI_DATA_DIR`, aby wybrać inne miejsce. W **Ustawieniach** sprawdzisz aktywną lokalizację i sposób ochrony danych logowania.

Jeśli to możliwe, magazyn danych uwierzytelniających używa klucza szyfrowania chronionego przez systemowy magazyn kluczy. Gdy magazyn kluczy jest niedostępny, Hycli wyraźnie informuje, że ochrona opiera się wyłącznie na uprawnieniach do plików. Dostęp do prywatnych katalogów i plików z danymi uwierzytelniającymi ma tylko bieżący użytkownik systemu. Istniejący zaszyfrowany magazyn nie jest zastępowany bez powiadomienia, gdy nie można go odblokować.

Żądania są wykonywane kolejno, w określonym tempie dla każdej strony i konta, z dodatkowym limitem dla całej strony. Hycli respektuje `Retry-After`, ogranicza rozmiary odpowiedzi i liczbę ponowień odczytu oraz wstrzymuje pracę przy błędach uwierzytelniania lub dodatkowej weryfikacji wymaganej przez stronę. Podczas przygotowywania strony Hycli nie zmienia kont, nie fałszuje cyfrowych odcisków ani nie omija zabezpieczeń; diagnostyka mechanizmu może działać inaczej. Te środki zmniejszają niepotrzebne obciążenie, ale nie gwarantują, że strona nigdy nie ograniczy konta.

Prywatne i lokalne adresy stron są domyślnie wyłączone. Dla zaufanej strony na własnym serwerze uruchom panel lub serwer MCP z jawną opcją `--allow-local`.

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

Testy używają symulowanych lokalnych stron i odpowiedzi dostawców. Sprawdzają powiązanie zgody z żądaniem i ochronę przed jej ponownym użyciem, ukrywanie sekretów, przekierowania, limity żądań, brak ponowień zapisu, ochronę sesji przeglądarki i zgodność odpowiedzi dostawców z oczekiwanym formatem. Nigdy nie twórz, nie zmieniaj ani nie usuwaj treści na rzeczywistym koncie wyłącznie po to, aby przetestować Hycli.

Zrzuty ekranu i GIF wykorzystują odizolowane dane przykładowe. Zobacz [źródła materiałów graficznych](../images/README.md).

## Licencja i przeznaczenie

Hycli jest udostępniany na licencji [Apache-2.0](../../LICENSE).

Hycli służy do tworzenia i używania narzędzi wiersza poleceń, które ułatwiają korzystanie ze stron internetowych. Używaj go ze stronami i kontami, do których masz uprawniony dostęp.

Oprogramowanie jest udostępniane w stanie, w jakim się znajduje, bez gwarancji. W zakresie dozwolonym przez prawo autorzy i współtwórcy nie odpowiadają za straty ani inne problemy wynikające z jego używania. Ponosisz odpowiedzialność za sposób korzystania z oprogramowania.

W zakresie dozwolonym przez prawo autorzy i współtwórcy nie ponoszą odpowiedzialności za blokady, zawieszenia lub ograniczenia kont wynikające z używania Hycli. Nie używaj Hycli do hakowania, uzyskiwania nieautoryzowanego dostępu ani przeprowadzania ataków.
