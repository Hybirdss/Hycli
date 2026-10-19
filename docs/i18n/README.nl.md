<p align="center">
  <img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="Hycli — Websites, klaar voor AI." width="100%" />
</p>

<p align="center">
  <strong>Verander elke website in een getypeerde CLI en MCP-server voor je AI-agent.</strong><br/>
  <sub>Lezen, zoeken, aanmaken, bewerken en automatiseren — met je echte browser-login, zonder dat er ooit cookies naar LLM's lekken en met ingebouwde anti-bot-bestendigheid.</sub>
</p>

<p align="center">
  <a href="https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0">Downloaden (Windows · macOS · Linux)</a> ·
  <a href="https://github.com/Hybirdss/Hycli/issues">Hulp krijgen op GitHub</a> ·
  <a href="#try-it-in-30-seconds">Probeer in 30 seconden</a> ·
  <a href="#get-started">Aan de slag</a> ·
  <a href="#what-your-ai-can-do">Wat je AI kan doen</a> ·
  <a href="#common-concerns">Veelgestelde vragen</a> ·
  <a href="#use-hycli-with-your-ai">Gebruik met je AI</a>
</p>

<details>
<summary>Lees in jouw taal · 20 talen</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

Geef Hycli een willekeurige website-URL en wat je wilt doen. Drie AI-workers inspecteren de documentatie, frontend-JS-bundels en het echte netwerkverkeer van de browser om een getypeerde CLI en MCP-toolset te bouwen — voor zowel lezen als schrijven — zonder tijdens het voorbereiden ooit iets in je account te wijzigen.

<p align="center">
  <img src="../../docs/images/dashboard.png" alt="Hycli-dashboard met websitekaarten, beschikbare acties en taakvoortgang." width="100%" />
</p>
<p align="center"><sub>Één lokaal dashboard en CLI voor je websites, accounts, schrijfrechten en resultaten.</sub></p>

<a id="try-it-in-30-seconds"></a>
## Probeer in 30 seconden

```sh
# 1. Wijs Hycli naar een website en beschrijf je doel (bereidt lezen & schrijven voor; nul live wijzigingen)
hycli prepare https://your-website.example --intent "Berichten zoeken, concepten opslaan en artikelen publiceren"

# 2. Bekijk alle voorbereide lees-, schrijf- en meerstaps-workflows
hycli describe your-website

# 3. Voer leesacties direct uit
hycli your-website search-posts --query "release notes"

# 4. Schakel schrijfrechten eenmalig in voor deze CLI wanneer je wilt aanmaken, bewerken of verwijderen
hycli writes your-website allow
hycli your-website create-draft --title "Hallo vanuit Hycli" --body "Geautomatiseerd vanuit mijn terminal."
```

Of koppel Hycli eenmalig aan **Claude Code, Cursor, Codex of Windsurf** met `"command": "hycli", "args": ["mcp"]` en vraag je agent in gewone taal:
> *"Gebruik Hycli om `https://...` voor te bereiden, laat zien welke acties beschikbaar zijn en stel een nieuw conceptbericht op."*

<a id="get-started"></a>
## Aan de slag

Hycli wordt geleverd als één enkel native binair bestand dat zowel de website-engine als het lokale dashboard (`http://127.0.0.1:4318`) bevat. Je hebt noch Rust noch Node.js nodig om een releasepakket uit te voeren.

### Downloaden en starten (Windows, macOS, Linux)

Download het native installatieprogramma of het draagbare archief voor jouw platform via **[GitHub Releases (v0.1.0)](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)**:

| Platform | Architectuur | Native installatieprogramma | Draagbaar archief | Hoe te starten |
| --- | --- | --- | --- | --- |
| **Windows 10/11** | x64 & ARM64 | `hycli-0.1.0-win32-*-setup.exe` | `hycli-0.1.0-win32-*.tar.gz` | Voer `*-setup.exe` uit (voegt `hycli` zonder beheerdersrechten toe aan je gebruikers-`PATH` en het Startmenu) en start vervolgens **Hycli** of voer `hycli open` uit in PowerShell. |
| **macOS 13+** | Apple Silicon (`arm64`) & Intel (`x64`) | `hycli-0.1.0-darwin-*.dmg` | `hycli-0.1.0-darwin-*.tar.gz` | Open de `.dmg`, sleep **Hycli.app** naar **Applications** en open de app (koppelt `hycli` automatisch in `~/.local/bin`). |
| **Linux** | x64 & ARM64 | `hycli-0.1.0-linux-*.deb` | `hycli-0.1.0-linux-*.tar.gz` | Installeer het `.deb`-bestand (`sudo apt install ./hycli-*.deb`) of pak het `.tar.gz`-archief uit en voer `./install.sh` uit, gevolgd door `hycli open`. |

<details>
<summary><strong>Snelle terminal-installatie & bouwen vanuit broncode</strong></summary>

**Draagbaar archief voor Linux / macOS:**

```sh
sha256sum -c hycli-0.1.0-*.tar.gz.sha256   # of op macOS: shasum -a 256 -c
tar -xzf hycli-0.1.0-*.tar.gz
cd hycli-0.1.0-*
./install.sh
hycli open
```

**Draagbaar archief voor Windows PowerShell:**

```powershell
tar -xzf hycli-0.1.0-win32-x64.tar.gz
cd hycli-0.1.0-win32-x64
powershell -ExecutionPolicy Bypass -File .\install.ps1
hycli open
```

**Bouwen vanuit broncode (Linux, macOS, Windows):**

Installeer de [bouwvereisten](../BUILD.md) en voer uit:

```sh
node scripts/build.mjs
./dist/hycli open
```

Voer `node scripts/package-desktop.mjs` uit om het native `.exe`-, `.dmg`- of `.deb`-installatieprogramma voor jouw besturingssysteem te maken. Zie [bouwen en verpakken](../BUILD.md) en [release-verificatie](../RELEASING.md).

</details>

### Verbind je eerste website

1. Voer `hycli open` uit (of klik op het **Hycli**-app-icoon) om het lokale dashboard te openen op `http://127.0.0.1:4318`.
2. Open **AI-verbindingen** en verbind OpenAI, Anthropic Claude, xAI, Z.ai of je lokale ChatGPT Codex-login.
3. Klik voor sites waarvoor je moet inloggen op **Accounts → Mijn aanmelding zoeken** om je browsersessie in de versleutelde lokale kluis te importeren.
4. Voer een website-URL en een optioneel doel in, of kopieer de MCP-configuratie vanuit **Codeer-agents** naar je AI-assistent.

<a id="what-your-ai-can-do"></a>
## Wat je AI kan doen

Hycli zet echte website-interfaces om in zelfdocumenterende CLI-subcommando's en MCP-tools:

| Website-interface | Voorbereide mogelijkheid |
| --- | --- |
| REST en OpenAPI | Getypeerde lees- en schrijfacties op basis van JSON/YAML-schema's en API-documentatie. |
| GraphQL | Geparseerde query- en mutatiedocumenten met getypeerde JSON-variabelen. |
| HTML- & JS-gerenderde pagina's | Gestructureerde records, links, paginering en gerenderde GET-leesacties via een lokale Chromium-engine. |
| JSON- en URL-gecodeerde formulieren | Gedocumenteerde invoervelden, standaardwaarden en accountgebonden formulierverzendingen (`POST`, `PUT`, `PATCH`, `DELETE`). |

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="Drie workers. Één heel druk klein vogeltje." width="100%" />
</p>
<p align="center"><sub>Drie workers. Één heel druk klein vogeltje.</sub></p>

| Actietype | Uitvoeringsgedrag |
| --- | --- |
| **Lezen of zoeken** | Wordt onmiddellijk uitgevoerd zodra deze door bewijs wordt ondersteund en als veilige leesactie is geclassificeerd. |
| **Aanmaken, verzenden, bewerken of verwijderen** | Wordt ontdekt en in de CLI opgenomen zonder ooit tijdens de voorbereiding te worden uitgevoerd. Standaard uitgeschakeld (`writes: false`). Je AI-agent vraagt je **eenmalig per CLI** of je schrijven wilt toestaan (`hycli writes SITE allow` of MCP `hycli_writes`). Zodra dit is ingeschakeld, worden schrijfacties direct uitgevoerd wanneer je daarom vraagt. |
| **Onduidelijk effect** | Wordt vastgehouden ter controle voordat er iets wordt verzonden. |
| **Authenticatie-uitdaging of snelheidslimiet** | Schakelt over op een andere transportstrategie (HTTP → `window.fetch` binnen de pagina → stealth Chromium) of pauzeert netjes. |

Raadpleeg het [operatie-overzicht](../SITESPEC.md) voor ondersteunde verzoekvormen en extractieregels.

<a id="common-concerns"></a>
## Veelgestelde vragen en hoe Hycli ze oplost

Een AI-agent verbinden met echte websites en ingelogde accounts roept serieuze vragen op. Zo lost Hycli elk punt door zijn ontwerp op:

### 1. "Zal de AI per ongeluk berichten plaatsen, bewerken of gegevens uit mijn account verwijderen?"
- **Nul live wijzigingen tijdens het maken van de CLI:** Tijdens het voorbereiden of controleren van een website (`hycli prepare`, `hycli request`, `hycli recon`, `hycli check`) ontdekt en compileert Hycli zowel lees- als schrijfbewerkingen in de CLI-specificatie, maar **voert nooit schrijfverzoeken uit**. Zelfs vermomde mutaties (`GET`-routes met wijzigende paden zoals `/delete`, GraphQL-`mutation`-bodies of HTML-formulieren met `<form method="post">`) worden tijdens de verificatie geblokkeerd.
- **Eenmalige schrijfbevestiging per CLI:** Elke nieuw aangemaakte website-CLI begint met `writes: false`. Wanneer een AI-agent voor het eerst een schrijfactie op een CLI moet uitvoeren, vraagt hij je **eenmalig voor die CLI**: *"Schrijfacties toestaan voor `<site>`?"* Pas na jouw bevestiging voert hij `hycli writes <site> allow` (of MCP `hycli_writes`) uit, waarmee directe schrijfuitvoering voor die CLI wordt ingeschakeld. Je kunt de toestemming op elk moment intrekken met `hycli writes <site> deny` of met de schakelaar **Wijzigingen toestaan** in het dashboard.
- **Geen automatische herhalingen bij schrijven:** Mislukte of onderbroken schrijfverzoeken worden nooit automatisch opnieuw geprobeerd. Als de verbinding wegvalt nadat een mutatie is verzonden, meldt Hycli `action_outcome_unknown`, zodat je eerst de website kunt controleren in plaats van een bestelling of bericht dubbel te plaatsen.

### 2. "Stuurt Hycli mijn cookies, inlogtokens of API-sleutels naar het AI-model?"
- **Geheimen komen nooit in LLM-prompts terecht:** Browsercookies, `localStorage`- / `sessionStorage`-tokens en API-sleutels worden uitsluitend opgeslagen in je lokale kluis (versleuteld via de sleutelhanger van het besturingssysteem in Windows Referentiebeheer, macOS Sleutelhanger of Linux Secret Service, of beperkt tot `0600`-rechten voor alleen de gebruiker).
- **Symbolische verbindingsrecepten (`ConnectionRecipe`):** AI-modellen zien alleen accountlabels, namen van opslagsleutels en gemaskeerde antwoordschema's. Het model bouwt een symbolische `ConnectionRecipe` (die verwijst naar een cookie- of tokennaam), en Hycli injecteert het echte geheim lokaal op het moment van uitvoering en verwijdert geheimen uit alle uitvoer en logboeken.
- **Site-specifieke alleen-lezen import:** Bij het importeren van een browsersessie worden alleen de cookies en opslag van het doeldomein gelezen uit een tijdelijke alleen-lezen momentopname. Het verzwakt nooit de browserbeveiliging en exporteert geen andere websites.

### 3. "Zorgt scraping voor CAPTCHA's of een verbanning van mijn account?"
- **Geen fuzzing of brute-force-scans:** Hycli raadt nooit lijsten met endpoints en voert geen fuzzing uit op live servers. Het leert uitsluitend van gekoppelde documentatie, OpenAPI-/GraphQL-schema's, statische JS-bundels en waargenomen netwerkverkeer in de browser.
- **Geserialiseerd, gedoseerd verkeer:** Verzoeken worden per website en per account geserialiseerd en in snelheid beperkt, waarbij `Retry-After` wordt gerespecteerd en herhaalde leesacties strikt worden begrensd.
- **Strategie-circuit-breaker & native browser-uitvoering:** Wanneer standaard HTTP op een 403/429-fout of een bot-uitdaging (Cloudflare, Akamai Bot Manager) stuit, blijft Hycli nooit hetzelfde geblokkeerde verzoek herhalen. De circuit-breaker schakelt onmiddellijk over op een ander transport — door `window.fetch(..., { credentials: 'include' })` uit te voeren binnen je ingelogde browsercontext of via een lokale stealth Chromium-engine — of pauzeert netjes om je account te beschermen.

### 4. "Kan een doorverwijzing mijn sessie lekken, of kan een gedeelde specificatie de bestandspaden van mijn computer prijsgeven?"
- **Strikte origin-binding:** Authenticatieheaders en cookies zijn vergrendeld op de doel-origin en worden bij cross-origin-doorverwijzingen onmiddellijk verwijderd. Privé- en loopback-IP-bereiken zijn standaard geblokkeerd, tenzij expliciet `--allow-local` wordt meegegeven.
- **Draagbare specificaties:** Lokale paden naar browser-uitvoeringsbestanden en profielen blijven in de lokale machinestatus en worden nooit weggeschreven naar draagbare `SiteSpec`-bestanden.

### 5. "Verspilt de AI tokens door te blijven proberen wanneer mijn sessie verloopt?"
- **Deterministische statuscontrole zonder AI (`hycli check`):** `hycli check` (en MCP `hycli_check`) speelt geverifieerde leesketens opnieuw af zonder een LLM aan te roepen en maakt een duidelijk onderscheid tussen `signed_out` (verbind je account opnieuw), `site_changed` (herstel de specificatie), `blocked` en `unreachable`.

<a id="ai-connections"></a>
## AI-verbindingen

| Provider | Authenticatie |
| --- | --- |
| ChatGPT · API-sleutel | OpenAI API-sleutel |
| Claude | Anthropic API-sleutel |
| xAI | xAI API-sleutel |
| Z.ai / Z.ai Coding Plan | Z.ai API-sleutel |
| ChatGPT · login | Lokale ChatGPT-aanmelding via het officiële Codex app-server-protocol |

De Codex-aanmeldingsverbinding draait in een tijdelijke thread waarin uitvoering van bestanden, shell, browser en externe MCP's is uitgeschakeld; Hycli leest of kopieert nooit Codex OAuth-tokens.

<a id="use-hycli-with-your-ai"></a>
## Gebruik Hycli met je AI

Voeg Hycli toe aan je MCP-client (**Codeer-agents → Verbindingsinstellingen bekijken** toont het exacte pad voor jouw besturingssysteem):

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

Gebruik `hycli mcp --sites-only` om alleen geïnstalleerde websitetools beschikbaar te stellen, of voeg `--only-site SITE` toe om het te beperken tot één enkele site. De MCP-server biedt `hycli_prepare`, `hycli_writes` (schakelt schrijven eenmalig per CLI in nadat je om toestemming is gevraagd), `hycli_run`, `hycli_check`, `hycli_result` en `hycli_activity` — en stelt nooit een tool beschikbaar om inloggegevens te lezen.

Vanuit de terminal werkt elke workflow op precies dezelfde manier:

```sh
hycli prepare https://your-website.example --intent "Opgeslagen referenties vinden"
hycli request SITE "Concepten bewerken en publiceren met volledige berichtinhoud toevoegen"
hycli describe SITE
hycli SITE ACTION --help
hycli writes SITE allow
hycli SITE ACTION --query "design systems"
hycli check SITE
```

[Agent-handleiding](../../agent/AGENTS.md) · [Hycli-skill](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## Browser-aanmelding

Hycli detecteert lokale profielen van Chrome, Chromium, Edge, Brave, Vivaldi, Arc, Whale en Firefox (plus lokale stealth Chromium-binaries wanneer geïnstalleerd) op Windows, macOS en Linux, en importeert alleen de sessie van de gevraagde website in de lokale kluis. Profielen die hetzelfde geverifieerde website-account delen, worden automatisch ontdubbeld.

Als een sessie is vergrendeld door Windows App-Bound-versleuteling, privécontainers of een niet-geïmporteerde browser, gebruik dan **Accounts → Een account verbinden** om via je browser in te loggen, koppel de [browser-companion-extensie](../../browser-companion/guide.html) of importeer een JSON-/Netscape-cookiebestand in het dashboard (plak nooit cookies in een AI-chat).

<a id="languages"></a>
## Talen

Het dashboard en de door AI gegenereerde actiebeschrijvingen ondersteunen de 20 talen die bovenaan deze README zijn gelinkt, inclusief rechts-naar-links Arabisch (RTL).

<a id="local-data"></a>
## Lokale gegevens en verzoeklimieten

Websitedefinities, versleutelde inloggegevens en activiteitslogboeken worden opgeslagen in de gegevensmap van je besturingssysteem (`%LOCALAPPDATA%\hycli` op Windows, `~/Library/Application Support/hycli` op macOS, `~/.local/share/hycli` op Linux of `HYCLI_DATA_DIR`). Bekijk de actieve kluisbeveiliging op elk moment onder **Instellingen**.

<a id="contributing"></a>
## Ontwikkeling en verificatie

```sh
npm --prefix dashboard ci
npm --prefix dashboard run check
npm --prefix dashboard run build
cargo test --all-targets --locked
cargo build --locked --bin hycli --example dashboard_fixture
node scripts/test-e2e.mjs --full
```

Alle tests draaien tegen synthetische lokale websites en fixtures — test schrijfbewerkingen nooit op een echt account van derden. Zie de [herkomst van visuele bronnen](../images/README.md).

## Licentie en beoogd gebruik

Hycli valt onder de [Apache-2.0](../../LICENSE)-licentie. Gebruik het uitsluitend met websites en accounts waartoe je gemachtigd bent. Geleverd "zoals het is" ("as is") zonder garantie; de auteurs en bijdragers zijn niet aansprakelijk voor verlies, accountbeperkingen of verbanningen die voortvloeien uit het gebruik ervan.

## Star History

<a href="https://www.star-history.com/#Hybirdss/Hycli&Date">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date&theme=dark" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
 </picture>
</a>
