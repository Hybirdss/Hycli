<p align="center">
  <img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="Hycli — Siti web pronti per l'IA." width="100%" />
</p>

<p align="center">
  <strong>Trasforma qualsiasi sito web in una CLI tipizzata e in un server MCP per il tuo agente IA.</strong><br/>
  <sub>Leggi, cerca, crea, modifica e automatizza usando il tuo vero accesso del browser, senza mai inviare cookie agli LLM e con resilienza anti-bot integrata.</sub>
</p>

<p align="center">
  <a href="https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0">Scarica (Windows · macOS · Linux)</a> ·
  <a href="https://github.com/Hybirdss/Hycli/issues">Ottieni aiuto su GitHub</a> ·
  <a href="#try-it-in-30-seconds">Provalo in 30 secondi</a> ·
  <a href="#get-started">Per iniziare</a> ·
  <a href="#what-your-ai-can-do">Cosa può fare la tua IA</a> ·
  <a href="#common-concerns">Domande frequenti</a> ·
  <a href="#use-hycli-with-your-ai">Usa con la tua IA</a>
</p>

<details>
<summary>Leggi nella tua lingua · 20 lingue</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

Fornisci a Hycli l'URL di qualsiasi sito web e ciò che vuoi fare. Tre worker IA analizzano la documentazione, i bundle JS frontend e il traffico di rete reale del browser per costruire una CLI tipizzata e un set di strumenti MCP — che coprono sia letture sia scritture — senza mai modificare il tuo account durante la preparazione.

<p align="center">
  <img src="../../docs/images/dashboard.png" alt="Dashboard di Hycli con schede dei siti web, azioni disponibili e avanzamento delle attività." width="100%" />
</p>
<p align="center"><sub>Un'unica dashboard locale e CLI per i tuoi siti web, account, permessi di scrittura e risultati.</sub></p>

<a id="try-it-in-30-seconds"></a>
## Provalo in 30 secondi

```sh
# 1. Indica a Hycli qualsiasi sito web e descrivi il tuo obiettivo (prepara letture e scritture; zero modifiche dal vivo)
hycli prepare https://your-website.example --intent "Cercare articoli, salvare bozze e pubblicare post"

# 2. Visualizza tutte le letture, le scritture e i flussi di lavoro multi-step preparati
hycli describe your-website

# 3. Esegui subito le azioni di lettura
hycli your-website search-posts --query "note di rilascio"

# 4. Abilita le scritture una sola volta per questa CLI quando sei pronto a creare, modificare o eliminare
hycli writes your-website allow
hycli your-website create-draft --title "Ciao da Hycli" --body "Automatizzato dal mio terminale."
```

Oppure collega Hycli a **Claude Code, Cursor, Codex o Windsurf** una sola volta con `"command": "hycli", "args": ["mcp"]` e chiedi al tuo agente in linguaggio naturale:
> *"Usa Hycli per preparare `https://...`, mostrami quali azioni sono disponibili e prepara la bozza di un nuovo articolo."*

<a id="get-started"></a>
## Per iniziare

Hycli è distribuito come singolo eseguibile nativo che contiene sia il motore per i siti web sia la dashboard locale (`http://127.0.0.1:4318`). Non servono né Rust né Node.js per eseguire un pacchetto di rilascio.

### Scarica ed esegui (Windows, macOS, Linux)

Scarica il programma di installazione nativo o l'archivio portatile per la tua piattaforma da **[GitHub Releases (v0.1.0)](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)**:

| Piattaforma | Architettura | Installer nativo | Archivio portatile | Come iniziare |
| --- | --- | --- | --- | --- |
| **Windows 10/11** | x64 e ARM64 | `hycli-0.1.0-win32-*-setup.exe` | `hycli-0.1.0-win32-*.tar.gz` | Esegui `*-setup.exe` (aggiunge `hycli` al `PATH` utente e al menu Start senza richiedere privilegi di amministratore), poi avvia **Hycli** o esegui `hycli open` in PowerShell. |
| **macOS 13+** | Apple Silicon (`arm64`) e Intel (`x64`) | `hycli-0.1.0-darwin-*.dmg` | `hycli-0.1.0-darwin-*.tar.gz` | Apri il file `.dmg`, trascina **Hycli.app** in **Applications** e aprilo (collega automaticamente `hycli` in `~/.local/bin`). |
| **Linux** | x64 e ARM64 | `hycli-0.1.0-linux-*.deb` | `hycli-0.1.0-linux-*.tar.gz` | Installa il `.deb` (`sudo apt install ./hycli-*.deb`) oppure estrai il `.tar.gz` ed esegui `./install.sh`, quindi avvia `hycli open`. |

<details>
<summary><strong>Installazione rapida da terminale e compilazione dai sorgenti</strong></summary>

**Archivio portatile per Linux / macOS:**

```sh
sha256sum -c hycli-0.1.0-*.tar.gz.sha256   # oppure su macOS: shasum -a 256 -c
tar -xzf hycli-0.1.0-*.tar.gz
cd hycli-0.1.0-*
./install.sh
hycli open
```

**Archivio portatile per Windows PowerShell:**

```powershell
tar -xzf hycli-0.1.0-win32-x64.tar.gz
cd hycli-0.1.0-win32-x64
powershell -ExecutionPolicy Bypass -File .\install.ps1
hycli open
```

**Compilazione dai sorgenti (Linux, macOS, Windows):**

Installa i [prerequisiti di compilazione](../BUILD.md) ed esegui:

```sh
node scripts/build.mjs
./dist/hycli open
```

Esegui `node scripts/package-desktop.mjs` per generare l'installer nativo `.exe`, `.dmg` o `.deb` per il tuo sistema operativo. Consulta [compilazione e pacchettizzazione](../BUILD.md) e [verifica dei rilasci](../RELEASING.md).

</details>

### Connetti il tuo primo sito web

1. Esegui `hycli open` (o fai clic sull'icona dell'app **Hycli**) per aprire la dashboard locale su `http://127.0.0.1:4318`.
2. Apri **Connessioni IA** e collega OpenAI, Anthropic Claude, xAI, Z.ai o il tuo login locale di ChatGPT Codex.
3. Per i siti che richiedono l'accesso, fai clic su **Account → Trova il mio accesso** per importare la sessione del browser nella cassaforte locale crittografata.
4. Inserisci l'URL di un sito web e un obiettivo facoltativo, oppure copia la configurazione MCP da **Agenti di programmazione** nel tuo assistente IA.

<a id="what-your-ai-can-do"></a>
## Cosa può fare la tua IA

Hycli trasforma le interfacce reali dei siti web in sottocomandi CLI autodocumentati e strumenti MCP:

| Interfaccia del sito web | Capacità preparata |
| --- | --- |
| REST e OpenAPI | Azioni di lettura e scrittura tipizzate basate su schemi JSON/YAML e documentazione API. |
| GraphQL | Documenti di query e mutation analizzati con variabili JSON tipizzate. |
| Pagine HTML e renderizzate via JS | Record strutturati, link, paginazione e letture GET renderizzate tramite un motore Chromium locale. |
| Moduli JSON e codificati in URL | Input documentati, valori predefiniti e invii di moduli associati all'account (`POST`, `PUT`, `PATCH`, `DELETE`). |

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="Tre worker. Un uccellino davvero indaffarato." width="100%" />
</p>
<p align="center"><sub>Tre worker. Un uccellino davvero indaffarato.</sub></p>

| Tipo di azione | Comportamento di esecuzione |
| --- | --- |
| **Lettura o ricerca** | Viene eseguita immediatamente quando è supportata da prove e classificata come lettura sicura. |
| **Creazione, invio, modifica o eliminazione** | Scoperta e sintetizzata nella CLI senza mai essere eseguita durante la preparazione. Disattivata per impostazione predefinita (`writes: false`). Il tuo agente IA ti chiede **una sola volta per CLI** se abilitare le scritture (`hycli writes SITE allow` o MCP `hycli_writes`). Una volta abilitate, le azioni di scrittura vengono eseguite direttamente quando le richiedi. |
| **Effetto non chiaro** | Trattenuta per revisione prima dell'invio. |
| **Sfida di autenticazione o limite di frequenza** | Cambia strategia di trasporto (HTTP → `window.fetch` nella pagina → Chromium stealth) oppure si mette in pausa in modo sicuro. |

Consulta il [riferimento delle operazioni](../SITESPEC.md) per i formati di richiesta supportati e le regole di estrazione.

<a id="common-concerns"></a>
## Domande frequenti e come Hycli le risolve

Collegare un agente IA a siti web reali e ad account autenticati solleva questioni importanti. Ecco come Hycli affronta ciascuna di esse fin dalla progettazione:

### 1. "L'IA pubblicherà, modificherà o eliminerà dati sul mio account per sbaglio?"
- **Zero modifiche dal vivo durante la creazione della CLI:** Durante la preparazione o la verifica di un sito web (`hycli prepare`, `hycli request`, `hycli recon`, `hycli check`), Hycli individua e compila sia le operazioni di lettura sia quelle di scrittura nella specifica della CLI, ma **non esegue mai richieste di scrittura**. Anche le modifiche camuffate (rotte `GET` con percorsi di modifica come `/delete`, corpi `mutation` GraphQL o moduli HTML `<form method="post">`) vengono bloccate durante la verifica.
- **Conferma di scrittura una tantum per CLI:** Ogni nuova CLI creata per un sito web parte con `writes: false`. Quando un agente IA deve eseguire per la prima volta un'azione di scrittura su una CLI, ti chiede **una sola volta per quella CLI**: *"Consentire le azioni di scrittura per `<site>`?"* Solo dopo la tua conferma esegue `hycli writes <site> allow` (o MCP `hycli_writes`), abilitando l'esecuzione diretta delle scritture per quella CLI. Puoi revocare il permesso in qualsiasi momento con `hycli writes <site> deny` o tramite l'interruttore **Consenti modifiche** nella dashboard.
- **Nessun nuovo tentativo automatico per le scritture:** Le richieste di scrittura fallite o interrotte non vengono mai ripetute automaticamente. Se la connessione cade dopo l'invio di una modifica, Hycli segnala `action_outcome_unknown` così puoi controllare il sito invece di duplicare un ordine o un post.

### 2. "Hycli invia i miei cookie, i token di accesso o le chiavi API al modello IA?"
- **I segreti non entrano mai nei prompt degli LLM:** I cookie del browser, i token `localStorage` / `sessionStorage` e le chiavi API sono conservati esclusivamente nella tua cassaforte locale (crittografata tramite il portachiavi di sistema su Gestione credenziali di Windows, Portachiavi macOS o Linux Secret Service, oppure limitata ai permessi utente `0600`).
- **Ricette di connessione simboliche (`ConnectionRecipe`):** I modelli IA vedono solo le etichette degli account, i nomi delle chiavi di archiviazione e gli schemi di risposta oscurati. Il modello costruisce una `ConnectionRecipe` simbolica (che fa riferimento al nome di un cookie o di un token), e Hycli inietta il segreto reale localmente al momento dell'esecuzione, rimuovendo i segreti da tutti gli output e dai log.
- **Importazione di sola lettura limitata al sito:** L'importazione di una sessione del browser legge solo i cookie e lo storage del dominio di destinazione da uno snapshot temporaneo di sola lettura. Non indebolisce mai la sicurezza del browser né esporta altri siti web.

### 3. "Lo scraping attiverà CAPTCHA o farà bannare il mio account?"
- **Nessun fuzzing né scansione brute-force:** Hycli non tira mai a indovinare liste di endpoint né esegue fuzzing sui server reali. Apprende dalla documentazione collegata, dagli schemi OpenAPI/GraphQL, dai bundle JS statici e dalle strutture di rete osservate nel browser.
- **Traffico serializzato e cadenzato:** Le richieste sono serializzate e limitate nella frequenza per sito web e per account, rispettando `Retry-After` e limitando i tentativi di lettura.
- **Circuit breaker di strategia ed esecuzione nativa nel browser:** Quando una normale richiesta HTTP incontra un errore 403/429 o una sfida anti-bot (Cloudflare, Akamai Bot Manager), Hycli non ripete mai all'infinito la richiesta bloccata. Il suo circuit breaker cambia immediatamente trasporto — eseguendo `window.fetch(..., { credentials: 'include' })` all'interno del contesto del tuo browser autenticato o usando un motore Chromium stealth locale — oppure si mette in pausa in modo pulito per proteggere il tuo account.

### 4. "Un reindirizzamento può far trapelare la mia sessione, o una specifica condivisa può esporre i percorsi del mio computer?"
- **Vincolo rigoroso all'origine:** Gli header di autenticazione e i cookie sono bloccati sull'origine di destinazione e vengono rimossi immediatamente in caso di reindirizzamenti cross-origin. Gli intervalli IP privati e di loopback sono bloccati per impostazione predefinita, a meno che non venga passato esplicitamente `--allow-local`.
- **Specifiche portatili:** I percorsi locali dell'eseguibile e dei profili del browser rimangono nello stato locale della macchina e non vengono mai scritti nei file `SiteSpec` portatili.

### 5. "L'IA sprecherà token riprovando quando scade la mia sessione?"
- **Controllo di stato deterministico senza IA (`hycli check`):** `hycli check` (e MCP `hycli_check`) riproduce le catene di lettura verificate senza invocare alcun LLM e distingue chiaramente `signed_out` (riconnetti il tuo account) da `site_changed` (ripara la specifica), `blocked` e `unreachable`.

<a id="ai-connections"></a>
## Connessioni IA

| Provider | Autenticazione |
| --- | --- |
| ChatGPT · chiave API | Chiave API OpenAI |
| Claude | Chiave API Anthropic |
| xAI | Chiave API xAI |
| Z.ai / Z.ai Coding Plan | Chiave API Z.ai |
| ChatGPT · accesso | Accesso locale a ChatGPT tramite il protocollo ufficiale Codex app-server |

La connessione di accesso tramite Codex viene eseguita in un thread effimero con esecuzione di file, shell, browser e MCP esterni disabilitata; Hycli non legge né copia mai i token OAuth di Codex.

<a id="use-hycli-with-your-ai"></a>
## Usa Hycli con la tua IA

Aggiungi Hycli al tuo client MCP (**Agenti di programmazione → Visualizza impostazioni di connessione** mostra il percorso esatto per il tuo sistema operativo):

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

Usa `hycli mcp --sites-only` per esporre solo gli strumenti dei siti web installati, oppure aggiungi `--only-site SITE` per limitare l'ambito a un singolo sito. Il server MCP fornisce `hycli_prepare`, `hycli_writes` (abilita le scritture una volta per CLI dopo averti chiesto conferma), `hycli_run`, `hycli_check`, `hycli_result` e `hycli_activity` — e non espone mai alcuno strumento per leggere le credenziali.

Dal terminale, ogni flusso di lavoro funziona esattamente allo stesso modo:

```sh
hycli prepare https://your-website.example --intent "Trovare i riferimenti salvati"
hycli request SITE "Aggiungere modifica e pubblicazione delle bozze con il contenuto completo dell'articolo"
hycli describe SITE
hycli SITE ACTION --help
hycli writes SITE allow
hycli SITE ACTION --query "design systems"
hycli check SITE
```

[Guida per gli agenti](../../agent/AGENTS.md) · [Skill di Hycli](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## Accesso tramite browser

Hycli rileva i profili locali di Chrome, Chromium, Edge, Brave, Vivaldi, Arc, Whale e Firefox (oltre ai binari locali di Chromium stealth quando installati) su Windows, macOS e Linux, importando nella cassaforte locale solo la sessione del sito web richiesto. I profili che condividono lo stesso account verificato di un sito vengono deduplicati automaticamente.

Se una sessione è protetta dalla crittografia App-Bound di Windows, da container privati o da un browser non importato, usa **Account → Connetti un account** per accedere tramite il browser, collega l'[estensione companion per il browser](../../browser-companion/guide.html) oppure importa un file di cookie JSON/Netscape nella dashboard (non incollare mai i cookie in una chat con l'IA).

<a id="languages"></a>
## Lingue

La dashboard e le descrizioni delle azioni generate dall'IA supportano le 20 lingue collegate in cima a questo README, incluso l'arabo da destra a sinistra (RTL).

<a id="local-data"></a>
## Dati locali e limiti delle richieste

Le definizioni dei siti web, le credenziali crittografate e i registri delle attività si trovano nella directory dati del tuo sistema operativo (`%LOCALAPPDATA%\hycli` su Windows, `~/Library/Application Support/hycli` su macOS, `~/.local/share/hycli` su Linux oppure `HYCLI_DATA_DIR`). Puoi verificare in qualsiasi momento lo stato di protezione della cassaforte in **Impostazioni**.

<a id="contributing"></a>
## Sviluppo e verifica

```sh
npm --prefix dashboard ci
npm --prefix dashboard run check
npm --prefix dashboard run build
cargo test --all-targets --locked
cargo build --locked --bin hycli --example dashboard_fixture
node scripts/test-e2e.mjs --full
```

Tutti i test vengono eseguiti su siti web locali sintetici e fixture: non testare mai operazioni di scrittura su un vero account di terze parti. Consulta la [provenienza delle risorse visive](../images/README.md).

## Licenza e uso previsto

Hycli è rilasciato sotto licenza [Apache-2.0](../../LICENSE). Utilizzalo esclusivamente con siti web e account a cui sei autorizzato ad accedere. Il software è fornito "così com'è", senza alcuna garanzia; gli autori e i collaboratori non sono responsabili per eventuali perdite, limitazioni dell'account o blocchi derivanti dal suo utilizzo.

## Star History

<a href="https://www.star-history.com/#Hybirdss/Hycli&Date">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date&theme=dark" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
 </picture>
</a>
