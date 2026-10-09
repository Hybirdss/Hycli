<p align="center">
  <img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="Hycli — Websites, klaar voor AI." width="100%" />
</p>

<p align="center">
  <strong>Maak van websites tools die je AI kan gebruiken.</strong>
</p>

<p align="center">
  <a href="#get-started">Aan de slag</a> ·
  <a href="#what-your-ai-can-do">Wat je AI kan doen</a> ·
  <a href="#ai-connections">AI-verbindingen</a> ·
  <a href="#use-hycli-with-your-ai">Gebruiken met je AI</a>
</p>

<details>
<summary>Lees in je eigen taal · 20 talen</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

Voeg een website toe. Hycli bereidt acties voor die je AI kan gebruiken en legt uit wat elke actie doet. Houd je websites, aanmeldingen en resultaten bij elkaar in een lokaal dashboard.

| Verbinden | Voorbereiden | Gebruiken |
| --- | --- | --- |
| Voeg een website toe en kies je AI. | Volg de voortgang en bekijk welke acties beschikbaar zijn. | Voer een actie uit of stel deze beschikbaar aan je AI-assistent. |

<p align="center">
  <img src="../../docs/images/dashboard.png" alt="Hycli-dashboard met websitekaarten, beschikbare acties en taakvoortgang." width="100%" />
</p>
<p align="center"><sub>Voorbeeldwerkruimte met voorbeeldtools en -accounts.</sub></p>

<a id="get-started"></a>
## Aan de slag

Nieuwe desktoppakketten: Windows (`*-setup.exe`), macOS (`.dmg`) en Linux (`.deb` of `./install.sh` na uitpakken). Open Hycli via het app-pictogram: de engine start op de achtergrond en opent de browser. Je hoeft geen terminal open te houden. Sluit af via Instellingen of gebruik `hycli open`, `hycli status` en `hycli stop`. Deze ontwikkelpakketten staan los van v0.1.0; bekijk platformcontroles en ondertekening in de [handleiding](../BUILD.md) en [CI](https://github.com/Hybirdss/Hycli/actions/workflows/verify.yml).

**[Downloaden v0.1.0 · Linux x64](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)** · glibc ≥ 2.39 · [SHA-256](https://github.com/Hybirdss/Hycli/releases/download/v0.1.0/hycli-0.1.0-linux-x64.tar.gz.sha256)

Hycli is een Rust-applicatie met een lokaal webdashboard. Bouw vanuit deze kopie van de repository het dashboard en het uitvoerbare bestand:

```sh
node scripts/build.mjs
./dist/hycli open
```

Het dashboard opent op `http://127.0.0.1:4318`. Gebruik `hycli dashboard --no-open` om het adres te tonen zonder een browser te openen, of `--port 4320` om een andere poort te kiezen. De website-engine en het dashboard zitten in hetzelfde uitvoerbare bestand.

1. Open **AI-verbindingen** en verbind een provider.
2. Voeg het adres van een website toe en kies de AI die deze gaat voorbereiden.
3. Bekijk de beschikbare acties. Voer er een uit in het dashboard of open **Codeeragenten** om de MCP-configuratie eenmalig te kopiëren.

Voor een website waarvoor je moet inloggen, verbind je het bijbehorende account via **Accounts**. Voor een website kun je meerdere accounts opslaan; je kiest zelf welk account de tools gebruiken.

<a id="what-your-ai-can-do"></a>
## Wat je AI kan doen

Hycli zet ondersteunde websitebewerkingen om in tools met een naam en vastgelegde typen. Door AI geschreven beschrijvingen leggen uit wat elke actie doet, welke informatie nodig is en wat de actie teruggeeft.

Drie onafhankelijke AI-agenten verzorgen leesacties en zoekopdrachten, nuttige werkstromen en accountinformatie. Het dashboard toont afgeronde stappen, de status van elke agent, de verstreken tijd en begrijpelijke werknotities. CLI- en MCP-activiteiten verschijnen in dezelfde activiteitengeschiedenis.

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="Drie workers. Eén heel druk vogeltje." width="100%" />
</p>
<p align="center"><sub>Drie workers. Eén heel druk vogeltje.</sub></p>

De voorbereiding gebruikt informatie die daadwerkelijk op de website beschikbaar is: gekoppelde documentatie, API-schema's, gepubliceerde JavaScript en verzoekstructuren die de browserextensie waarneemt. Er kunnen beperkte leesacties worden uitgevoerd om een bewerking te controleren. Er worden geen testrecords aangemaakt, inhoud bewerkt, objecten verwijderd, lange lijsten met endpoints geraden of fuzzingtests op een actieve server uitgevoerd.

| Actie | Gedrag |
| --- | --- |
| Lezen of zoeken | Wordt zelfstandig uitgevoerd wanneer de bewerking door bewijs wordt ondersteund en als leesactie is geclassificeerd. |
| Aanmaken, verzenden, bewerken of verwijderen | Toont de website, het account, de actie en de ingevulde invoerwaarden ter goedkeuring door de gebruiker. |
| Onduidelijk effect | Vereist controle voordat het verzoek wordt verzonden. |
| Authenticatie, beveiligingscontrole of verzoeklimiet | Pauzeert de betreffende verzoeken en laat je opnieuw verbinden of wachten. |

Goedkeuring geldt voor één exact verzoek, verloopt na vijf minuten en kan slechts eenmaal worden gebruikt. Een wijziging in de invoer, het account, de opgeslagen inloggegevens of de geïnstalleerde tooldefinitie maakt de goedkeuring ongeldig. Uitvoering via de CLI, MCP en het dashboard volgt dezelfde regels. Schrijfacties worden nooit automatisch opnieuw geprobeerd.

De ondersteuning hangt af van de website. Een pagina zonder bruikbare documentatie of waargenomen bewerkingen heeft mogelijk een ingelogde browser, een aangeleverde SiteSpec of extra voorbereiding nodig. Hycli meldt wat het kan ondersteunen, zonder te beweren dat elke website een kant-en-klare API heeft.

### Ondersteunde functies

Ondersteunt OpenAPI in JSON of YAML, REST, GraphQL en JSON- of URL-gecodeerde formulieren. HTML-records, links en volgende pagina’s worden uitgelezen met waargenomen selectors; GET-pagina’s kunnen zo nodig met lokaal Chromium worden gerenderd. Mislukt een leesactie tijdens het voorbereiden, dan bekijkt de AI het bewijs, past de definitie aan en controleert die opnieuw.

Willekeurige klikreeksen, multipart-uploads en binaire downloads zijn nog niet geïmplementeerd. Pakketten bevatten geen accounts of eerder gegenereerde CLI’s.

[Actieformaat](../SITESPEC.md) · [Pakketcontrole](../RELEASING.md)

<a id="ai-connections"></a>
## AI-verbindingen

| Verbinding | Aanmelden |
| --- | --- |
| ChatGPT · API key | Je OpenAI-API-sleutel |
| Claude | Je Anthropic-API-sleutel |
| xAI | Je xAI-API-sleutel |
| Z.ai | Je Z.ai-API-sleutel |
| Z.ai Coding Plan | Je Z.ai Coding Plan-API-sleutel |
| ChatGPT · login | ChatGPT-aanmelding beheerd door de geïnstalleerde Codex app-server |

Kies een model in het dashboard, ook een model dat specifiek voor jouw account beschikbaar is. Verbindingscontroles verifiëren de provider en het gekozen model. API-providers brengen het gebruik in rekening bij je account bij die provider.

De Codex-verbinding gebruikt het officiële app-server-protocol. Hycli leest of kopieert geen Codex-OAuth-tokens. Inferentie vindt plaats in een tijdelijke thread waarin bestandsbewerkingen en uitvoering via shell, browser, applicaties en MCP zijn uitgeschakeld. De gecontroleerde leestools van Hycli verzorgen de voorbereiding van websites.

<a id="use-hycli-with-your-ai"></a>
## Hycli gebruiken met je AI

**Codeeragenten → Verbindingsinstellingen bekijken**

De algemene MCP-verbinding omvat het voorbereiden van websites en het uitvoeren van acties. Verbind je agent eenmaal, zodat deze ontbrekende tools kan voorbereiden, de voortgang kan volgen en nieuwe acties kan gebruiken.

```sh
hycli mcp
```

Gebruik `--sites-only` om alleen geïnstalleerde acties beschikbaar te stellen. De onderstaande verbinding is beperkt tot één website.

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

Staat `hycli` niet in het PATH van de agent, gebruik dan het programmapad dat het dashboard toont.

Dezelfde werkwijze is beschikbaar via de CLI. Vervang de voorbeeld-URL, `SITE` en `ACTION` door je website en de namen die `describe` teruggeeft.

```sh
hycli prepare https://your-website.example --intent "Opgeslagen bronnen vinden"
hycli request SITE "Add draft editing and publishing with full post content"
hycli describe
hycli describe SITE ACTION
hycli SITE ACTION --help
hycli run SITE ACTION --arg query="design systems"
hycli jobs show JOB_ID --watch
```

Geef in MCP de URL en `intent` door aan `hycli_prepare` en volg de taak met `hycli_job` of `hycli_result`. `hycli_run` kan nieuwe acties uitvoeren voordat de client zijn toollijst vernieuwt. Zoek interne ID’s eerst op via bijbehorende lijst- of zoekacties.

Wijzigingen leveren een ontvangstbewijs op voor beoordeling in het dashboard. Volg het resultaat zonder het verzoek te herhalen. Een mislukte leesactie of onverwacht antwoord leidt ook in de CLI tot een foutstatus.

[Agenthandleiding](../../agent/AGENTS.md) · [Hycli-skill](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## Aanmelden via de browser

De voorbereiding van een website begint met het controleren van het huidige besturingssysteem, actieve en geregistreerde browsers en beschikbare profielen. De AI kiest uit die mogelijkheden om documentatie te lezen, pagina’s weer te geven, een websitesessie te importeren en de verbinding te verifiëren. Browser- en profielpaden blijven op de lokale computer; een gegenereerde websitedefinitie is niet afhankelijk van de installatiepaden van de ontwikkelaar.

Cookies en opslag per origin uit Chrome, Chromium, Edge, Brave en Firefox kunnen worden geïmporteerd als ze toegankelijk zijn. Alleen de sessie van de aangevraagde website komt in de lokale kluis van Hycli. Hycli kiest automatisch één geverifieerde identiteit; profielen van diezelfde identiteit tellen als één accountkeuze. Een bestaande accountkeuze blijft behouden; verschillende geverifieerde identiteiten vereisen een keuze.

Bij **Accounts → Account verbinden** start **Mijn aanmelding zoeken** de zoektocht opnieuw. Als er geen bruikbare sessie wordt gevonden, opent **Inlogpagina openen** de website in je standaardbrowser. Hycli controleert opnieuw zolang het dialoogvenster open is en gaat pas verder met de voorbereiding nadat het gekozen account is geverifieerd. Alleen een tabblad openen geldt niet als geslaagde aanmelding.

Door het besturingssysteem beschermde, browsergebonden, privé- of containersessies kunnen de Hycli-browserextensie vereisen. Kies het tabblad voor de extensie, maak een verbindingscode aan, open de extensie in het aangemelde profiel, voer de code in en geef toegang tot die website. De gewone import omzeilt de Windows App-Bound-beveiliging niet en verzwakt een browserprofiel niet.

- **Chrome en Edge:** pak het pakket uit en gebruik **Uitgepakte extensie laden** op de extensiepagina van de browser.
- **Firefox:** gebruik het Firefox-pakket en **Tijdelijke add-on laden** in `about:debugging`. Tijdelijke extensies worden verwijderd wanneer Firefox opnieuw opstart. Deze versie bevat geen ondertekende distributie via extensiewinkels.
- **Cookiebestand:** als alternatief kun je cookiebestanden in JSON- en Netscape-formaat importeren. Importeer het bestand rechtstreeks in het dashboard; plak de inhoud niet in een AI-gesprek.

De browser draagt sessiegegevens rechtstreeks over naar de lokale kluis voor aanmeldgegevens. Modellen ontvangen accountlabels, namen en structuur van lokale sleutels en toolresultaten zonder waarden van aanmeldgegevens. Ze kunnen een verbindingsrecept maken dat naar die lokale waarden verwijst zonder de waarden te ontvangen. Bereik, paden, vervaldatum en ondersteunde partitiegegevens van cookies worden gerespecteerd; authenticatieheaders blijven beperkt tot hun oorspronkelijke origin.

De accountidentiteit komt uit een daadwerkelijke reactie van de website over het huidige account. De naam van een browserprofiel wordt niet beschouwd als een geverifieerde website-identiteit. Als het account nog niet is geïdentificeerd, meldt Hycli dat. Na het verbinden kan normaal browsen een accountreactie en de structuren van beschikbare verzoeken opleveren, zonder verzoekwaarden, headers of antwoordinhoud aan het voorbereidingsmodel bloot te stellen.

Een site kan afzonderlijke, gedocumenteerde API-aanmeldgegevens of een lokaal niet beschikbare browserfunctie vereisen. De voorbereiding legt de werkelijke resultaten van verbinden en lezen vast. Een opgehaalde startpagina of een inlogpagina als API-antwoord betekent niet dat de integratie klaar is. De [handleiding voor browserinstellingen](../../browser-companion/guide.html) beschrijft de extensiestappen en beperkingen.

<a id="languages"></a>
## Talen

Het dashboard ondersteunt de 20 hierboven gelinkte talen, waaronder Arabisch met schrijfrichting van rechts naar links. Engels is de standaardtaal bij de eerste start. Je keuze wordt op dit apparaat opgeslagen en AI-beschrijvingen kunnen in de gekozen taal worden voorbereid.

<a id="local-data"></a>
## Lokale gegevens en verzoeklimieten

Websitedefinities, accountmetadata en activiteiten blijven in de lokale gegevensmap. Stel `HYCLI_DATA_DIR` in om een andere locatie te kiezen. Via **Instellingen** bekijk je de actieve locatie en de bescherming van inloggegevens.

De kluis voor inloggegevens gebruikt een encryptiesleutel die door de sleutelring van het besturingssysteem wordt beschermd, als deze beschikbaar is. Als er geen sleutelring beschikbaar is, wordt expliciet gemeld dat de opslag uitsluitend door bestandsrechten wordt beschermd; privémappen en bestanden met inloggegevens zijn beperkt tot de huidige gebruiker van het besturingssysteem. Een bestaande versleutelde kluis wordt niet stilzwijgend vervangen als deze niet kan worden ontgrendeld.

Verzoeken worden per website en account achter elkaar en met tussenpozen uitgevoerd, met daarnaast een budget voor de hele website. Hycli respecteert `Retry-After`, begrenst antwoordgroottes en herhaalde leespogingen en pauzeert bij authenticatiefouten of beveiligingscontroles van de website. De voorbereiding van websites wisselt niet van account, vervalst geen fingerprints en omzeilt geen beveiligingscontroles om aan een beperking te ontsnappen. Deze maatregelen verminderen vermijdbare belasting; ze kunnen niet garanderen dat een website een account nooit beperkt.

Privé- en lokale websiteadressen zijn standaard uitgeschakeld. Start voor een vertrouwde, zelfgehoste website het dashboard of de MCP-server expliciet met `--allow-local`.

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

Tests gebruiken synthetische lokale websites en providerreacties. Ze controleren het binden van goedkeuringen en het voorkomen van hergebruik, het afschermen van geheimen, omleidingen, verzoeklimieten, het uitblijven van herhaalde schrijfpogingen, bescherming van browsersessies en de afgesproken antwoordstructuren van providers. Gebruik nooit een echt account om inhoud aan te maken, te wijzigen of te verwijderen alleen om Hycli te testen.

De screenshots en GIF gebruiken geïsoleerde voorbeeldgegevens. Bekijk de [beeldbronnen](../images/README.md).

## Licentie en beoogd gebruik

Hycli valt onder de [Apache-2.0](../../LICENSE)-licentie.

Hycli is bedoeld voor het maken en gebruiken van opdrachtregeltools die websites eenvoudiger in gebruik maken. Gebruik het met websites en accounts waartoe je toegang mag hebben.

De software wordt geleverd zoals deze is, zonder garantie. Voor zover wettelijk toegestaan zijn de auteurs en bijdragers niet aansprakelijk voor verliezen of andere problemen die voortvloeien uit het gebruik. Je bent zelf verantwoordelijk voor hoe je de software gebruikt.

Voor zover wettelijk toegestaan, zijn de auteurs en bijdragers niet aansprakelijk voor het blokkeren, schorsen of beperken van accounts als gevolg van het gebruik van Hycli. Gebruik Hycli niet voor hacken, ongeoorloofde toegang of aanvallen.
