<p align="center">
  <img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="Hycli — Websites, bereit für KI." width="100%" />
</p>

<p align="center">
  <strong>Verwandle jede Website in eine typisierte CLI und einen MCP-Server für deinen KI-Agenten.</strong><br/>
  <sub>Lesen, suchen, erstellen, bearbeiten und automatisieren – mit deinem echten Browser-Login, ohne Cookie-Lecks an LLMs und mit integriertem Anti-Bot-Schutz.</sub>
</p>

<p align="center">
  <a href="https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0">Herunterladen (Windows · macOS · Linux)</a> ·
  <a href="https://github.com/Hybirdss/Hycli/issues">Hilfe auf GitHub erhalten</a> ·
  <a href="#try-it-in-30-seconds">In 30 Sekunden testen</a> ·
  <a href="#get-started">Erste Schritte</a> ·
  <a href="#what-your-ai-can-do">Was deine KI tun kann</a> ·
  <a href="#common-concerns">Häufige Bedenken</a> ·
  <a href="#use-hycli-with-your-ai">Mit deiner KI nutzen</a>
</p>

<details>
<summary>In deiner Sprache lesen · 20 Sprachen</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

Gib Hycli eine beliebige Website-URL und dein Ziel an. Drei KI-Worker analysieren die Dokumentation, die Frontend-JS-Bundles und den echten Netzwerkverkehr im Browser, um eine typisierte CLI und ein MCP-Toolset zu erstellen – für Lese- und Schreibaktionen gleichermaßen –, ohne dein Konto während der Einrichtung jemals zu verändern.

<p align="center">
  <img src="../../docs/images/dashboard.png" alt="Hycli-Dashboard mit Website-Karten, verfügbaren Aktionen und Aufgabenfortschritt." width="100%" />
</p>
<p align="center"><sub>Ein einziges lokales Dashboard und eine CLI für deine Websites, Konten, Schreibberechtigungen und Ergebnisse.</sub></p>

<a id="try-it-in-30-seconds"></a>
## In 30 Sekunden testen

```sh
# 1. Gib Hycli eine Website und dein Ziel an (bereitet Lesen & Schreiben vor; null Live-Änderungen)
hycli prepare https://your-website.example --intent "Beiträge suchen, Entwürfe speichern und Artikel veröffentlichen"

# 2. Alle vorbereiteten Lese-, Schreib- und mehrstufigen Workflows anzeigen
hycli describe your-website

# 3. Leseaktionen sofort ausführen
hycli your-website search-posts --query "Release Notes"

# 4. Schreibzugriff einmalig für diese CLI freigeben, wenn du erstellen, bearbeiten oder löschen möchtest
hycli writes your-website allow
hycli your-website create-draft --title "Hallo von Hycli" --body "Direkt aus meinem Terminal automatisiert."
```

Oder verbinde Hycli einmalig mit **Claude Code, Cursor, Codex oder Windsurf** über `"command": "hycli", "args": ["mcp"]` und beauftrage deinen Agenten in natürlicher Sprache:
> *„Nutze Hycli, um `https://...` vorzubereiten, zeige mir die verfügbaren Aktionen und erstelle einen neuen Entwurf.“*

<a id="get-started"></a>
## Erste Schritte

Hycli wird als einzelne native Binärdatei ausgeliefert, die sowohl die Website-Engine als auch das lokale Dashboard (`http://127.0.0.1:4318`) enthält. Weder Rust noch Node.js werden benötigt, um ein Release-Paket auszuführen.

### Herunterladen und starten (Windows, macOS, Linux)

Lade das native Installationsprogramm oder das portable Archiv für deine Plattform unter **[GitHub Releases (v0.1.0)](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)** herunter:

| Plattform | Architektur | Natives Installationsprogramm | Portables Archiv | So startest du |
| --- | --- | --- | --- | --- |
| **Windows 10/11** | x64 & ARM64 | `hycli-0.1.0-win32-*-setup.exe` | `hycli-0.1.0-win32-*.tar.gz` | `*-setup.exe` ausführen (fügt `hycli` ohne Admin-Rechte zum Benutzer-`PATH` und Startmenü hinzu), dann **Hycli** starten oder in PowerShell `hycli open` ausführen. |
| **macOS 13+** | Apple Silicon (`arm64`) & Intel (`x64`) | `hycli-0.1.0-darwin-*.dmg` | `hycli-0.1.0-darwin-*.tar.gz` | `.dmg` öffnen, **Hycli.app** in **Applications** ziehen und öffnen (verknüpft `hycli` automatisch in `~/.local/bin`). |
| **Linux** | x64 & ARM64 | `hycli-0.1.0-linux-*.deb` | `hycli-0.1.0-linux-*.tar.gz` | `.deb` installieren (`sudo apt install ./hycli-*.deb`) oder `.tar.gz` entpacken und `./install.sh` starten, danach `hycli open` ausführen. |

<details>
<summary><strong>Schnellinstallation im Terminal & aus dem Quellcode bauen</strong></summary>

**Portables Archiv für Linux / macOS:**

```sh
sha256sum -c hycli-0.1.0-*.tar.gz.sha256   # oder unter macOS: shasum -a 256 -c
tar -xzf hycli-0.1.0-*.tar.gz
cd hycli-0.1.0-*
./install.sh
hycli open
```

**Portables Archiv für Windows PowerShell:**

```powershell
tar -xzf hycli-0.1.0-win32-x64.tar.gz
cd hycli-0.1.0-win32-x64
powershell -ExecutionPolicy Bypass -File .\install.ps1
hycli open
```

**Aus dem Quellcode bauen (Linux, macOS, Windows):**

Installiere die [Build-Voraussetzungen](../BUILD.md) und führe Folgendes aus:

```sh
node scripts/build.mjs
./dist/hycli open
```

Führe `node scripts/package-desktop.mjs` aus, um das native `.exe`-, `.dmg`- oder `.deb`-Installationsprogramm für dein Betriebssystem zu erstellen. Weitere Details findest du unter [Bauen und Paketieren](../BUILD.md) sowie [Release-Verifizierung](../RELEASING.md).

</details>

### Deine erste Website verbinden

1. Führe `hycli open` aus (oder klicke auf das **Hycli**-App-Symbol), um das lokale Dashboard unter `http://127.0.0.1:4318` zu öffnen.
2. Öffne **KI-Verbindungen** und verbinde OpenAI, Anthropic Claude, xAI, Z.ai oder deinen lokalen ChatGPT-Codex-Login.
3. Klicke bei Websites mit Anmeldung auf **Konten → Meine Anmeldung suchen**, um deine Browser-Sitzung in den verschlüsselten lokalen Tresor zu importieren.
4. Gib eine Website-URL und optional ein Ziel ein oder kopiere die MCP-Konfiguration unter **Coding-Agenten** in deinen KI-Assistenten.

<a id="what-your-ai-can-do"></a>
## Was deine KI tun kann

Hycli verwandelt echte Website-Schnittstellen in selbstdokumentierte CLI-Unterbefehle und MCP-Tools:

| Website-Schnittstelle | Vorbereitete Fähigkeit |
| --- | --- |
| REST und OpenAPI | Typisierte Lese- und Schreibaktionen auf Basis von JSON/YAML-Schemata und API-Dokumentation. |
| GraphQL | Analysierte Query- und Mutation-Dokumente mit typisierten JSON-Variablen. |
| HTML- & JS-gerenderte Seiten | Strukturierte Datensätze, Links, Paginierung und gerenderte GET-Abfragen über eine lokale Chromium-Engine. |
| JSON- und URL-kodierte Formulare | Dokumentierte Eingabefelder, Standardwerte und kontogebundene Formular-Sendeaktionen (`POST`, `PUT`, `PATCH`, `DELETE`). |

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="Drei Worker. Ein sehr fleißiger kleiner Vogel." width="100%" />
</p>
<p align="center"><sub>Drei Worker. Ein sehr fleißiger kleiner Vogel.</sub></p>

| Aktionstyp | Ausführungsverhalten |
| --- | --- |
| **Lesen oder suchen** | Wird sofort ausgeführt, sobald die Aktion durch Belege abgesichert und als sicherer Lesezugriff eingestuft ist. |
| **Erstellen, senden, bearbeiten oder löschen** | Wird während der Vorbereitung erkannt und in die CLI eingebaut, aber dabei niemals ausgeführt. Standardmäßig deaktiviert (`writes: false`). Dein KI-Agent fragt dich **einmalig pro CLI**, ob Schreibaktionen erlaubt werden sollen (`hycli writes SITE allow` oder MCP `hycli_writes`). Nach der Freigabe werden Schreibaktionen auf deine Anweisung hin direkt ausgeführt. |
| **Unklare Auswirkung** | Wird vor dem Senden zur Überprüfung zurückgehalten. |
| **Auth-Challenge oder Rate-Limit** | Wechselt die Transportstrategie (HTTP → seiteninternes `window.fetch` → Stealth-Chromium) oder pausiert sauber. |

Unterstützte Anfrageformate und Extraktionsregeln findest du in der [Operationsreferenz](../SITESPEC.md).

<a id="common-concerns"></a>
## Häufige Bedenken und wie Hycli sie löst

Einen KI-Agenten mit echten Websites und angemeldeten Konten zu verbinden, wirft wichtige Sicherheitsfragen auf. So löst Hycli jede davon durch seine Architektur:

### 1. „Wird die KI versehentlich Beiträge veröffentlichen, bearbeiten oder Daten in meinem Konto löschen?“
- **Null Live-Änderungen während der CLI-Erstellung:** Beim Vorbereiten oder Prüfen einer Website (`hycli prepare`, `hycli request`, `hycli recon`, `hycli check`) erkennt und kompiliert Hycli sowohl Lese- als auch Schreiboperationen in die CLI-Spezifikation, **führt aber niemals Schreibanfragen aus**. Selbst getarnte Änderungen (`GET`-Routen mit verändernden Pfaden wie `/delete`, GraphQL-`mutation`-Bodies oder HTML-Formulare mit `<form method="post">`) werden während der Verifizierung blockiert.
- **Einmalige Schreibbestätigung pro CLI:** Jede neu erstellte Website-CLI startet mit `writes: false`. Wenn ein KI-Agent zum ersten Mal eine Schreibaktion auf einer CLI ausführen soll, fragt er dich **einmalig für diese CLI**: *„Schreibaktionen für `<site>` erlauben?“* Erst nach deiner Bestätigung führt er `hycli writes <site> allow` (oder MCP `hycli_writes`) aus und schaltet die direkte Schreibausführung für diese CLI frei. Du kannst die Berechtigung jederzeit mit `hycli writes <site> deny` oder über den Schalter **Änderungen erlauben** im Dashboard widerrufen.
- **Keine automatischen Wiederholungen bei Schreibzugriffen:** Fehlgeschlagene oder abgebrochene Schreibanfragen werden niemals automatisch wiederholt. Bricht die Verbindung nach dem Senden einer Änderung ab, meldet Hycli `action_outcome_unknown`, damit du die Website prüfen kannst, statt eine Bestellung oder einen Beitrag doppelt abzuschicken.

### 2. „Sendet Hycli meine Cookies, Login-Tokens oder API-Schlüssel an das KI-Modell?“
- **Geheimnisse gelangen niemals in LLM-Prompts:** Browser-Cookies, `localStorage`- / `sessionStorage`-Tokens und API-Schlüssel werden ausschließlich in deinem lokalen Tresor gespeichert (verschlüsselt über den Betriebssystem-Schlüsselbund in der Windows-Anmeldeinformationsverwaltung, dem macOS-Schlüsselbund oder dem Linux Secret Service bzw. geschützt durch `0600`-Benutzerrechte).
- **Symbolische Verbindungsrezepte (`ConnectionRecipe`):** KI-Modelle sehen nur Kontobezeichnungen, Speicherschlüsselnamen und geschwärzte Antwortschemata. Das Modell erstellt ein symbolisches `ConnectionRecipe` (das lediglich auf einen Cookie- oder Token-Namen verweist), und Hycli fügt das echte Geheimnis erst lokal zur Ausführungszeit ein und entfernt Geheimnisse aus allen Ausgaben und Protokollen.
- **Website-gebundener, schreibgeschützter Import:** Beim Importieren einer Browser-Sitzung werden nur die Cookies und Speicherdaten der Zieldomain aus einem temporären, schreibgeschützten Snapshot gelesen. Die Browser-Sicherheit wird niemals geschwächt und andere Websites werden nicht exportiert.

### 3. „Löst das Scraping CAPTCHAs aus oder führt es zu einer Kontosperrung?“
- **Kein Fuzzing oder Brute-Force-Scanning:** Hycli errät niemals Endpunkt-Wortlisten und führt kein Fuzzing auf Live-Servern durch. Es lernt ausschließlich aus verlinkter Dokumentation, OpenAPI-/GraphQL-Schemata, statischen JS-Bundles und beobachtetem Netzwerkverkehr im Browser.
- **Serialisierter, gedrosselter Datenverkehr:** Anfragen werden pro Website und Konto serialisiert und im Tempo begrenzt; `Retry-After`-Header werden eingehalten und Lese-Wiederholungen strikt begrenzt.
- **Strategie-Circuit-Breaker & native Browser-Ausführung:** Wenn einfaches HTTP auf einen 403/429-Status oder eine Bot-Challenge (Cloudflare, Akamai Bot Manager) stößt, wiederholt Hycli niemals endlos dieselbe blockierte Anfrage. Der Circuit-Breaker wechselt sofort den Transportweg – führt `window.fetch(..., { credentials: 'include' })` direkt in deinem authentifizierten Browser-Kontext aus oder nutzt eine lokale Stealth-Chromium-Engine – oder pausiert sauber, um dein Konto zu schützen.

### 4. „Kann eine Weiterleitung meine Sitzung verraten oder eine geteilte Spezifikation lokale Dateipfade offenlegen?“
- **Strikte Origin-Bindung:** Authentifizierungs-Header und Cookies sind fest an die Ziel-Origin gebunden und werden bei Cross-Origin-Weiterleitungen sofort entfernt. Private und Loopback-IP-Bereiche sind standardmäßig blockiert, sofern nicht ausdrücklich `--allow-local` angegeben wird.
- **Portable Spezifikationen:** Lokale Browser-Programm- und Profilpfade verbleiben ausschließlich im lokalen Gerätezustand und werden niemals in portable `SiteSpec`-Dateien geschrieben.

### 5. „Verschwendet die KI Tokens durch ständiges Wiederholen, wenn meine Sitzung abläuft?“
- **Deterministische Statusprüfung ohne KI (`hycli check`):** `hycli check` (und MCP `hycli_check`) spielt verifizierte Leseketten ab, ohne ein LLM aufzurufen, und unterscheidet sauber zwischen `signed_out` (Konto neu verbinden), `site_changed` (Spezifikation reparieren), `blocked` und `unreachable`.

<a id="ai-connections"></a>
## KI-Verbindungen

| Anbieter | Authentifizierung |
| --- | --- |
| ChatGPT · API-Schlüssel | OpenAI-API-Schlüssel |
| Claude | Anthropic-API-Schlüssel |
| xAI | xAI-API-Schlüssel |
| Z.ai / Z.ai Coding Plan | Z.ai-API-Schlüssel |
| ChatGPT · Login | Lokale ChatGPT-Anmeldung über das offizielle Codex-App-Server-Protokoll |

Die Codex-Login-Verbindung läuft in einem kurzlebigen Thread, in dem Datei-, Shell-, Browser- und externe MCP-Ausführungen deaktiviert sind; Hycli liest oder kopiert niemals Codex-OAuth-Tokens.

<a id="use-hycli-with-your-ai"></a>
## Hycli mit deiner KI nutzen

Füge Hycli zu deinem MCP-Client hinzu (**Coding-Agenten → Verbindungseinstellungen anzeigen** zeigt den genauen Pfad für dein Betriebssystem):

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

Verwende `hycli mcp --sites-only`, um nur installierte Website-Tools bereitzustellen, oder füge `--only-site SITE` hinzu, um den Zugriff auf eine einzelne Website zu beschränken. Der MCP-Server stellt `hycli_prepare`, `hycli_writes` (aktiviert Schreibzugriffe einmalig pro CLI nach Rückfrage), `hycli_run`, `hycli_check`, `hycli_result` und `hycli_activity` bereit – und bietet niemals ein Tool zum Auslesen von Zugangsdaten an.

Im Terminal funktioniert jeder Workflow auf genau dieselbe Weise:

```sh
hycli prepare https://your-website.example --intent "Gespeicherte Referenzen finden"
hycli request SITE "Entwurfsbearbeitung und Veröffentlichung mit vollständigem Beitragstext hinzufügen"
hycli describe SITE
hycli SITE ACTION --help
hycli writes SITE allow
hycli SITE ACTION --query "design systems"
hycli check SITE
```

[Agenten-Leitfaden](../../agent/AGENTS.md) · [Hycli-Skill](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## Browser-Anmeldung

Hycli erkennt lokale Profile von Chrome, Chromium, Edge, Brave, Vivaldi, Arc, Whale und Firefox (sowie installierte lokale Stealth-Chromium-Binärdateien) unter Windows, macOS und Linux und importiert ausschließlich die Sitzung der angeforderten Website in den lokalen Tresor. Profile, die dasselbe verifizierte Website-Konto nutzen, werden automatisch zusammengeführt.

Wenn eine Sitzung durch Windows App-Bound-Verschlüsselung, private Container oder einen nicht importierten Browser gesperrt ist, nutze **Konten → Konto verbinden**, um dich im Browser anzumelden, verbinde die [Browser-Begleiterweiterung](../../browser-companion/guide.html) oder importiere eine JSON-/Netscape-Cookie-Datei im Dashboard (füge Cookies niemals in einen KI-Chat ein).

<a id="languages"></a>
## Sprachen

Das Dashboard und die KI-generierten Aktionsbeschreibungen unterstützen alle 20 oben in diesem README verlinkten Sprachen, einschließlich rechtsläufigem Arabisch (RTL).

<a id="local-data"></a>
## Lokale Daten und Anfragelimits

Website-Definitionen, verschlüsselte Zugangsdaten und Aktivitätsprotokolle liegen im Datenverzeichnis deines Betriebssystems (`%LOCALAPPDATA%\hycli` unter Windows, `~/Library/Application Support/hycli` unter macOS, `~/.local/share/hycli` unter Linux oder `HYCLI_DATA_DIR`). Den aktiven Tresorschutz kannst du jederzeit unter **Einstellungen** einsehen.

<a id="contributing"></a>
## Entwicklung und Verifizierung

```sh
npm --prefix dashboard ci
npm --prefix dashboard run check
npm --prefix dashboard run build
cargo test --all-targets --locked
cargo build --locked --bin hycli --example dashboard_fixture
node scripts/test-e2e.mjs --full
```

Alle Tests laufen gegen synthetische lokale Websites und Fixtures – teste Schreiboperationen niemals gegen ein echtes Drittanbieter-Konto. Siehe [Herkunft der Bildmaterialien](../images/README.md).

## Lizenz und bestimmungsgemäße Verwendung

Hycli steht unter der [Apache-2.0](../../LICENSE)-Lizenz. Verwende es nur mit Websites und Konten, zu deren Zugriff du berechtigt bist. Die Bereitstellung erfolgt „wie besehen“ ohne Gewährleistung; die Autoren und Mitwirkenden haften nicht für Verluste, Kontoeinschränkungen oder Sperrungen, die aus der Nutzung entstehen.

## Star History

<a href="https://www.star-history.com/#Hybirdss/Hycli&Date">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date&theme=dark" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
 </picture>
</a>
