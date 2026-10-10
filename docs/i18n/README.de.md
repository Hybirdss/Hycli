<p align="center">
  <img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="Hycli — Websites, bereit für KI." width="100%" />
</p>

<p align="center">
  <strong>Automatisiere jede Website.</strong>
</p>

<p align="center">
  <a href="#get-started">Erste Schritte</a> ·
  <a href="#what-your-ai-can-do">Was deine KI tun kann</a> ·
  <a href="#ai-connections">KI-Verbindungen</a> ·
  <a href="#use-hycli-with-your-ai">Mit deiner KI verwenden</a>
</p>

<details>
<summary>In deiner Sprache lesen · 20 Sprachen</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

Füge eine Website hinzu. Hycli bereitet Aktionen vor, die deine KI nutzen kann, und erklärt, was jede davon tut. Behalte Websites, Anmeldungen und Ergebnisse in einem lokalen Dashboard im Blick.

| Verbinden | Vorbereiten | Verwenden |
| --- | --- | --- |
| Füge eine Website hinzu und wähle deine KI. | Verfolge den Fortschritt und sieh dir verfügbare Aktionen an. | Führe eine Aktion aus oder stelle sie deinem KI-Assistenten bereit. |

<p align="center">
  <img src="../../docs/images/dashboard.png" alt="Hycli-Dashboard mit Website-Karten, verfügbaren Aktionen und Aufgabenfortschritt." width="100%" />
</p>
<p align="center"><sub>Beispiel-Workspace mit Beispiel-Tools und -Konten.</sub></p>

<a id="get-started"></a>
Geben Sie eine Domain und daneben optional einen Zweck ein. Die KI versteht zuerst die Website und wählt nützliche Abläufe aus. Anschließend prüft sie Voraussetzungen, Eingaben und Endergebnisse. Fehlende Schritte bleiben als prüfbedürftig markiert.

## Erste Schritte

**[Herunterladen v0.1.0 · Linux x64](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)** · glibc ≥ 2.39 · [SHA-256](https://github.com/Hybirdss/Hycli/releases/download/v0.1.0/hycli-0.1.0-linux-x64.tar.gz.sha256)

Hycli ist eine Rust-Anwendung mit einem lokalen Web-Dashboard. Baue aus diesem ausgecheckten Repository das Dashboard und die ausführbare Datei:

```sh
node scripts/build.mjs
./dist/hycli dashboard
```

Das Dashboard öffnet sich unter `http://127.0.0.1:4318`. Mit `hycli dashboard --no-open` wird die Adresse ausgegeben, ohne einen Browser zu öffnen. Mit `--port 4320` wählst du einen anderen Port. Die Website-Engine und das Dashboard sind in derselben ausführbaren Datei enthalten.

1. Öffne **KI-Verbindungen** und verbinde einen Anbieter.
2. Füge eine Website-Adresse hinzu und wähle die KI, die sie vorbereiten soll.
3. Prüfe die verfügbaren Aktionen. Führe eine im Dashboard aus oder öffne **Coding-Agents**, um die MCP-Konfiguration einmalig zu kopieren.

Für eine Website mit Anmeldung verbindest du das zugehörige Konto unter **Konten**. Für eine Website lassen sich mehrere Konten speichern; du entscheidest, welches ihre Werkzeuge verwenden.

<a id="what-your-ai-can-do"></a>
## Was deine KI tun kann

Hycli macht unterstützte Website-Operationen zu benannten, typisierten Werkzeugen. Von der KI verfasste Beschreibungen erklären, was jede Aktion bewirkt, welche Informationen sie benötigt und was sie zurückgibt.

Drei unabhängige KI-Agenten kümmern sich um Lesezugriffe und Suche, nützliche Arbeitsabläufe sowie Kontoinformationen. Das Dashboard zeigt abgeschlossene Schritte, den Status jedes Agenten, die verstrichene Zeit und verständliche Arbeitsnotizen an. CLI- und MCP-Aktionen erscheinen im selben Aktivitätsverlauf.

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="Drei Worker. Ein sehr beschäftigtes kleines Vögelchen." width="100%" />
</p>
<p align="center"><sub>Drei Worker. Ein sehr beschäftigtes kleines Vögelchen.</sub></p>

Die Vorbereitung nutzt tatsächlich verfügbare Informationen der Website: verlinkte Dokumentation, API-Schemata, veröffentlichtes JavaScript und von der Browser-Erweiterung beobachtete Anfragestrukturen. Nach den geplanten Workflows arbeiten die KI-Agenten jede dokumentierte API-Operation ohne Aktion ab, bis jede umgesetzt oder mit Begründung gemeldet ist; das `coverage` des Auftrags listet auf, was noch offen ist. Sie kann begrenzte Lesezugriffe durchführen, um eine Operation zu prüfen. Dabei werden keine Testdatensätze erstellt, Inhalte bearbeitet, Objekte gelöscht, lange Listen von Endpunkten erraten oder Fuzzing-Tests auf einem laufenden Server ausgeführt.

| Aktion | Verhalten |
| --- | --- |
| Lesen oder suchen | Wird selbstständig ausgeführt, wenn die Operation durch Belege gestützt und als Lesezugriff eingestuft ist. |
| Erstellen, senden, bearbeiten oder löschen | Ist aus, bis du Änderungen für diese Website erlaubst. Danach zeigt es Website, Konto, Aktion und aufgelöste Eingabewerte zur Freigabe durch dich an. |
| Unklare Wirkung | Erfordert eine Prüfung, bevor die Anfrage gesendet wird. |
| Authentifizierung, Sicherheitsprüfung oder Anfragelimit | Pausiert betroffene Anfragen und lässt dich erneut verbinden oder warten. |

Jede Website startet schreibgeschützt. Ihre Änderungsaktionen sind in MCP ausgeblendet und werden von der CLI abgelehnt, bis du in den Details der Website **Änderungen erlauben** einschaltest; beim Ausschalten werden offene Freigaben abgebrochen. Der Schalter und die Freigabe-Schaltfläche existieren nur im Dashboard, und das Dashboard erteilt seine Sitzung nur der Adresse, die `hycli dashboard` öffnet. Ein Agent, der Hycli aufruft, kann Änderungen also weder einschalten noch seine eigene Anfrage freigeben. Ein Agent, der beliebige Befehle als dein Nutzer ausführen kann, kann auch deine Dateien lesen; lass solche Agenten bei ihren eigenen Rückfragen zur Berechtigung.

Eine Freigabe gilt für genau eine Anfrage, läuft nach fünf Minuten ab und kann nur einmal verwendet werden. Änderungen an Eingaben, Konto, gespeicherten Anmeldedaten oder installierter Werkzeugdefinition machen sie ungültig. Ausführungen über CLI, MCP und Dashboard unterliegen denselben Regeln. Schreibzugriffe werden niemals automatisch wiederholt.

Die Unterstützung hängt von der Website ab. Eine Seite ohne nutzbare Dokumentation oder beobachtete Operationen benötigt möglicherweise einen angemeldeten Browser, eine bereitgestellte SiteSpec oder weitere Vorbereitung. Hycli gibt an, was es unterstützen kann, statt zu behaupten, jede Website habe eine fertige API.

### Unterstützte Funktionen

Unterstützt werden OpenAPI in JSON oder YAML, REST, GraphQL sowie JSON- und URL-kodierte Formularanfragen. HTML-Datensätze, Links und Folgeseiten lassen sich über beobachtete Selektoren auslesen; GET-Seiten können bei Bedarf mit lokalem Chromium gerendert werden. Schlägt ein Lesezugriff bei der Vorbereitung fehl, prüft die KI die Belege, korrigiert die Definition und testet sie erneut.

Beliebige Klickabläufe, Multipart-Uploads und binäre Downloads sind noch nicht implementiert. Pakete enthalten keine Konten oder zuvor erzeugten CLI-Definitionen.

[Aktionsformat](../SITESPEC.md) · [Paketprüfung](../RELEASING.md)

<a id="ai-connections"></a>
## KI-Verbindungen

| Verbindung | Anmeldung |
| --- | --- |
| ChatGPT · API key | Dein OpenAI-API-Schlüssel |
| Claude | Dein Anthropic-API-Schlüssel |
| xAI | Dein xAI-API-Schlüssel |
| Z.ai | Dein Z.ai-API-Schlüssel |
| Z.ai Coding Plan | Dein Z.ai Coding Plan-API-Schlüssel |
| ChatGPT · login | ChatGPT-Anmeldung, verwaltet vom installierten Codex app-server |

Wähle im Dashboard ein Modell, auch eines, das speziell deinem Konto zur Verfügung steht. Verbindungsprüfungen prüfen den Anbieter und das ausgewählte Modell. API-Anbieter rechnen die Nutzung über dein Konto beim jeweiligen Anbieter ab.

Die Codex-Verbindung nutzt das offizielle app-server-Protokoll. Hycli liest oder kopiert keine Codex-OAuth-Tokens. Die Inferenz läuft in einem kurzlebigen Thread, in dem Dateioperationen sowie Shell-, Browser-, Anwendungs- und MCP-Ausführung deaktiviert sind. Die Website-Vorbereitung erfolgt über die kontrollierten Lesewerkzeuge von Hycli.

<a id="use-hycli-with-your-ai"></a>
## Hycli mit deiner KI verwenden

**Coding-Agenten → Verbindungseinstellungen ansehen**

Die allgemeine MCP-Verbindung umfasst das Vorbereiten von Websites und das Ausführen ihrer Aktionen. Verbinde deinen Agenten einmal, damit er fehlende Werkzeuge vorbereiten, den Fortschritt verfolgen und neue Aktionen nutzen kann.

```sh
hycli mcp
```

Mit `--sites-only` werden nur installierte Aktionen angeboten. Die folgende Verbindung ist auf eine Website beschränkt.

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

Wenn `hycli` nicht im PATH des Agenten liegt, verwende den im Dashboard angezeigten Programmpfad.

Derselbe Ablauf ist über die CLI verfügbar. Ersetze die Beispiel-URL sowie `SITE` und `ACTION` durch deine Website und die von `describe` ausgegebenen Namen.

```sh
hycli prepare https://your-website.example --intent "Gespeicherte Quellen finden"
hycli request SITE "Add draft editing and publishing with full post content"
hycli describe
hycli describe SITE ACTION
hycli SITE ACTION --help
hycli run SITE ACTION --arg query="design systems"
hycli jobs show JOB_ID --watch
```

Prüfe die Website ohne KI, bevor du eine fehlgeschlagene Aktion wiederholst:

```sh
hycli check          # jede installierte Website
hycli check SITE
```

Die Vorbereitung zeichnet die erfolgreichen Lesezugriffe der Reihe nach auf, auch solche, die die Kennung eines früheren Ergebnisses an den nächsten weitergeben. `hycli check` spielt sie erneut ab und bestätigt zuerst die Identität des gespeicherten Kontos. Das Ergebnis unterscheidet `signed_out` (Konto erneut verbinden) von `site_changed` (das Konto funktioniert, aber die Antworten passen nicht mehr; Reparatur anfordern) sowie `blocked` und `unreachable`. Es führt nie eine Änderung aus, und der Exit-Status folgt dem Ergebnis. MCP-Clients erhalten dieselbe Prüfung als `hycli_check`.

Übergib in MCP die URL und `intent` an `hycli_prepare` und verfolge den Auftrag mit `hycli_job` oder `hycli_result`. `hycli_run` führt neue Aktionen auch aus, bevor der Client seine Werkzeugliste aktualisiert hat. Ermittle interne IDs zuerst über passende Listen- oder Suchaktionen. `hycli_result`, `hycli_check` und `hycli_activity` bleiben in beiden Modi verfügbar, damit deine KI Arbeitsnotizen, Fortschritt und freigegebene Ergebnisse verfolgen und eine abgelaufene Anmeldung von einer geänderten Website unterscheiden kann.

Änderungen liefern einen Beleg zur Prüfung im Dashboard. Verfolge dessen Ergebnis, ohne die Anfrage erneut zu senden. Fehlgeschlagene Lesezugriffe und unerwartete Antworten führen auch in der CLI zu einem Fehlerstatus.

[Agentenleitfaden](../../agent/AGENTS.md) · [Hycli-Skill](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## Anmeldung über den Browser

Die Vorbereitung einer Website beginnt mit der Prüfung des aktuellen Betriebssystems, laufender und registrierter Browser sowie verfügbarer Profile. Die KI wählt aus diesen Möglichkeiten, um Dokumentation zu lesen, Seiten darzustellen, eine Website-Sitzung zu importieren und die Verbindung zu prüfen. Browser- und Profilpfade bleiben auf dem lokalen Rechner; eine erzeugte Website-Definition hängt nicht von den Installationspfaden des Entwicklers ab.

Cookies und Origin-Speicher aus Chrome, Chromium, Edge, Brave und Firefox können importiert werden, sofern sie zugänglich sind. Nur die Sitzung der angeforderten Website gelangt in den lokalen Tresor von Hycli. Hycli wählt automatisch eine einzige verifizierte Identität; Profile derselben Identität zählen bei der Kontoauswahl als ein Konto. Eine vorhandene Kontoauswahl bleibt erhalten; unterschiedliche verifizierte Identitäten erfordern eine Auswahl.

Unter **Konten → Konto verbinden** startet **Meine Anmeldung finden** die Suche erneut. Wenn keine verwendbare Sitzung gefunden wird, öffnet **Anmeldeseite öffnen** die Website in deinem Standardbrowser. Hycli prüft erneut, solange der Dialog geöffnet ist, und setzt die Vorbereitung erst fort, wenn das ausgewählte Konto verifiziert wurde. Ein geöffneter Tab allein gilt nicht als erfolgreiche Anmeldung.

Durch das Betriebssystem geschützte, browsergebundene, private Sitzungen oder Container-Sitzungen können die Hycli-Browsererweiterung benötigen. Wähle den Erweiterungs-Tab, erstelle einen Verbindungscode, öffne die Erweiterung im angemeldeten Profil, gib den Code ein und erlaube den Zugriff auf diese Website. Der native Import umgeht den Windows-App-Bound-Schutz nicht und schwächt kein Browserprofil.

- **Chrome und Edge:** Entpacke das Paket und wähle auf der Erweiterungsseite des Browsers **Entpackte Erweiterung laden**.
- **Firefox:** Nutze das Firefox-Paket und **Temporäres Add-on laden** unter `about:debugging`. Temporäre Erweiterungen werden beim Neustart von Firefox entfernt. Diese Version enthält keine signierte Verteilung über Erweiterungsstores.
- **Cookie-Datei:** Als Alternative können Cookie-Dateien im JSON- oder Netscape-Format importiert werden. Importiere die Datei direkt im Dashboard; füge ihren Inhalt nicht in ein KI-Gespräch ein.

Der Browser überträgt Sitzungsdaten direkt in den lokalen Tresor für Zugangsdaten. Modelle erhalten Kontobezeichnungen, lokale Schlüsselnamen und Strukturen sowie Werkzeugergebnisse ohne Zugangsdatenwerte. Sie können ein Verbindungsrezept erstellen, das auf diese lokalen Werte verweist, ohne sie zu erhalten. Geltungsbereich, Pfade, Ablaufdatum und unterstützte Partitionierungsinformationen der Cookies werden berücksichtigt; Authentifizierungsheader bleiben auf ihren ursprünglichen Origin beschränkt.

Die Kontoidentität stammt aus einer tatsächlichen Antwort der Website zum aktuellen Konto. Der Name eines Browserprofils gilt nicht als verifizierte Website-Identität. Wenn das Konto noch nicht identifiziert wurde, weist Hycli darauf hin. Nach dem Verbinden kann normales Surfen eine Kontoantwort und die Strukturen verfügbarer Anfragen liefern, ohne dem Vorbereitungsmodell Anfragewerte, Header oder Antwortinhalte offenzulegen.

Eine Website benötigt möglicherweise separate dokumentierte API-Zugangsdaten oder eine lokal nicht verfügbare Browserfunktion. Die Vorbereitung zeichnet die tatsächlichen Verbindungs- und Leseergebnisse auf. Das Abrufen einer Startseite oder eine von einer API zurückgegebene Anmeldeseite bedeutet nicht, dass die Integration bereit ist. Die [Anleitung zur Browser-Einrichtung](../../browser-companion/guide.html) erklärt den Ablauf mit der Erweiterung und dessen Grenzen.

<a id="languages"></a>
## Sprachen

Das Dashboard unterstützt die oben verlinkten 20 Sprachen, darunter Arabisch mit Schreibrichtung von rechts nach links. Die Ausgangssprache ist Englisch. Deine Wahl wird auf diesem Gerät gespeichert, und KI-Beschreibungen können in der gewählten Sprache erstellt werden.

<a id="local-data"></a>
## Lokale Daten und Anfragelimits

Website-Definitionen, Kontometadaten und Aktivitäten bleiben im lokalen Datenverzeichnis. Mit `HYCLI_DATA_DIR` wählst du einen anderen Speicherort. Unter **Einstellungen** siehst du den aktiven Speicherort und den Schutz der Zugangsdaten.

Der Zugangsdaten-Tresor verwendet einen Verschlüsselungsschlüssel, der vom Schlüsselbund des Betriebssystems geschützt wird, sofern dieser verfügbar ist. Ohne Schlüsselbund wird ausdrücklich angezeigt, dass der Speicher nur durch Dateiberechtigungen geschützt ist; private Verzeichnisse und Zugangsdaten-Dateien sind auf den aktuellen Betriebssystemnutzer beschränkt. Ein bestehender verschlüsselter Tresor wird nicht stillschweigend ersetzt, wenn er sich nicht entsperren lässt.

Anfragen werden pro Website und Konto nacheinander und mit zeitlichen Abständen ausgeführt; zusätzlich gilt ein Budget für die gesamte Website. Hycli berücksichtigt `Retry-After`, begrenzt Antwortgrößen und Wiederholungen von Lesezugriffen und pausiert bei Authentifizierungsfehlern oder Sicherheitsprüfungen der Website. Die Website-Vorbereitung wechselt keine Konten, fälscht keine Fingerabdrücke und umgeht keine Sicherheitsprüfungen, um Beschränkungen zu vermeiden. Diese Maßnahmen verringern vermeidbare Belastung; sie können nicht garantieren, dass eine Website ein Konto niemals einschränkt.

Private und lokale Website-Adressen sind standardmäßig deaktiviert. Für eine vertrauenswürdige selbst gehostete Website startest du das Dashboard oder den MCP-Server ausdrücklich mit `--allow-local`.

<a id="contributing"></a>
## Entwicklung und Überprüfung

```sh
npm --prefix dashboard ci
npm --prefix dashboard run check
npm --prefix dashboard run build
cargo test --all-targets --locked
cargo build --locked --bin hycli --example dashboard_fixture
node scripts/test-e2e.mjs --full
```

Tests verwenden synthetische lokale Websites und Anbieterantworten. Sie prüfen die Bindung von Freigaben und den Schutz vor Wiederverwendung, das Entfernen von Geheimnissen, Weiterleitungen, Ratenlimits, das Ausbleiben erneuter Schreibversuche, den Schutz von Browsersitzungen und die Antwortverträge der Anbieter. Verwende niemals ein echtes Konto, um Inhalte nur zum Testen von Hycli zu erstellen, zu ändern oder zu löschen.

Screenshots und GIF verwenden isolierte Beispieldaten. Siehe die [Bildquellen](../images/README.md).

## Lizenz und vorgesehene Nutzung

Hycli steht unter der Lizenz [Apache-2.0](../../LICENSE).

Hycli ist dafür vorgesehen, Kommandozeilenwerkzeuge zu erstellen und zu nutzen, die die Bedienung von Websites erleichtern. Verwende es mit Websites und Konten, für die du eine Zugriffsberechtigung hast.

Die Software wird ohne Gewährleistung im vorliegenden Zustand bereitgestellt. Soweit gesetzlich zulässig, haften ihre Autoren und Mitwirkenden nicht für Verluste oder andere Probleme, die durch ihre Nutzung entstehen. Du bist für deine Nutzung verantwortlich.

Soweit gesetzlich zulässig, haften die Autoren und Mitwirkenden nicht für Kontosperrungen, Kontosuspendierungen oder Kontoeinschränkungen infolge der Nutzung von Hycli. Verwende Hycli nicht zum Hacken, für unbefugten Zugriff oder für Angriffe.
