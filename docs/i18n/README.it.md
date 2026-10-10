<p align="center">
  <img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="Hycli — Siti web pronti per l’IA." width="100%" />
</p>

<p align="center">
  <strong>Trasforma i siti web in strumenti che la tua IA può usare.</strong>
</p>

<p align="center">
  <a href="#get-started">Primi passi</a> ·
  <a href="#what-your-ai-can-do">Cosa può fare la tua IA</a> ·
  <a href="#ai-connections">Connessioni IA</a> ·
  <a href="#use-hycli-with-your-ai">Usa con la tua IA</a>
</p>

<details>
<summary>Leggi nella tua lingua · 20 lingue</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

Aggiungi un sito web. Hycli prepara azioni che la tua IA può usare e spiega cosa fa ciascuna. Riunisci siti web, accessi e risultati in una dashboard locale.

| Connetti | Prepara | Usa |
| --- | --- | --- |
| Aggiungi un sito web e scegli la tua IA. | Segui l'avanzamento e scopri quali azioni sono disponibili. | Esegui un'azione o mettila a disposizione del tuo assistente IA. |

<p align="center">
  <img src="../../docs/images/dashboard.png" alt="Dashboard Hycli con schede dei siti web, azioni disponibili e avanzamento delle attività." width="100%" />
</p>
<p align="center"><sub>Spazio di lavoro di esempio con strumenti e account dimostrativi.</sub></p>

<a id="get-started"></a>
Inserisci un dominio e, facoltativamente, uno scopo nel campo accanto. L’IA prima comprende il sito e sceglie flussi utili, poi ne verifica prerequisiti, dati e risultati. Se mancano passaggi, il sito resta da verificare.

## Primi passi

**[Scarica v0.1.0 · Linux x64](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)** · glibc ≥ 2.39 · [SHA-256](https://github.com/Hybirdss/Hycli/releases/download/v0.1.0/hycli-0.1.0-linux-x64.tar.gz.sha256)

Hycli è un'applicazione Rust con una dashboard web locale. Da questa copia del repository, compila la dashboard e l'eseguibile:

```sh
node scripts/build.mjs
./dist/hycli dashboard
```

La dashboard si apre su `http://127.0.0.1:4318`. Usa `hycli dashboard --no-open` per stampare l'indirizzo senza aprire il browser, oppure `--port 4320` per scegliere un'altra porta. Il motore per i siti web e la dashboard sono inclusi nello stesso eseguibile.

1. Apri **Connessioni IA** e connetti un fornitore.
2. Aggiungi l'indirizzo di un sito web e scegli l'IA che lo preparerà.
3. Esamina le azioni disponibili. Eseguine una nella dashboard oppure apri **Agenti di programmazione** per copiare una sola volta la configurazione MCP.

Per un sito che richiede l'accesso, collega il relativo account da **Account**. Un sito può avere più account salvati; scegli tu quale devono usare i suoi strumenti.

<a id="what-your-ai-can-do"></a>
## Cosa può fare la tua IA

Hycli trasforma le operazioni supportate dai siti web in strumenti con un nome e tipi definiti. Le descrizioni scritte dall'IA spiegano cosa fa ogni azione, quali informazioni richiede e cosa restituisce.

Tre agenti IA indipendenti gestiscono letture e ricerche, flussi di lavoro utili e informazioni sugli account. La dashboard mostra le fasi completate, lo stato di ciascun agente, il tempo trascorso e note di lavoro in linguaggio chiaro. Le attività della CLI e di MCP compaiono nella stessa cronologia.

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="Tre worker. Un uccellino molto indaffarato." width="100%" />
</p>
<p align="center"><sub>Tre worker. Un uccellino molto indaffarato.</sub></p>

La preparazione si basa su informazioni effettivamente disponibili sul sito: documentazione collegata, schemi API, JavaScript pubblicato e strutture delle richieste osservate dall'estensione del browser. Può eseguire letture circoscritte per verificare un'operazione. Non crea record di prova, modifica contenuti, elimina oggetti, cerca di indovinare lunghe liste di endpoint o esegue fuzzing su un server attivo.

| Azione | Comportamento |
| --- | --- |
| Leggere o cercare | Viene eseguita autonomamente quando l'operazione è sostenuta da riscontri e classificata come lettura. |
| Creare, inviare, modificare o eliminare | Mostra il sito web, l'account, l'azione e i valori di input risolti per l'approvazione dell'utente. |
| Effetto incerto | Richiede una verifica prima di inviare la richiesta. |
| Autenticazione, verifica o limite di richieste | Sospende le richieste interessate e consente di riconnettersi o attendere. |

L'approvazione si applica a una richiesta precisa, scade dopo cinque minuti e può essere usata una sola volta. Modificare gli input, l'account, i dati di accesso salvati o la definizione dello strumento installato la invalida. L'esecuzione tramite CLI, MCP e dashboard rispetta gli stessi vincoli. Le scritture non vengono mai ritentate automaticamente.

Il supporto dipende dal sito web. Una pagina senza documentazione utilizzabile o operazioni osservate può richiedere un browser con accesso già effettuato, un SiteSpec fornito o un'ulteriore preparazione. Hycli indica cosa può supportare, senza affermare che ogni sito abbia un'API pronta all'uso.

### Funzioni supportate

Supporta OpenAPI JSON o YAML, REST, GraphQL e richieste JSON o con moduli codificati per URL. Il contenuto HTML può essere estratto come record, link e pagina successiva tramite selettori osservati; si possono leggere anche pagine GET renderizzate con Chromium locale. Se una lettura fallisce durante la preparazione, l’IA esamina le evidenze, corregge la definizione e la verifica di nuovo.

Le sequenze arbitrarie di clic, i caricamenti multipart e i download binari non sono ancora implementati. I pacchetti non includono account né CLI generate in precedenza.

[Contratto delle operazioni](../SITESPEC.md) · [Verifica dei pacchetti](../RELEASING.md)

<a id="ai-connections"></a>
## Connessioni IA

| Connessione | Accesso |
| --- | --- |
| ChatGPT · API key | La tua chiave API OpenAI |
| Claude | La tua chiave API Anthropic |
| xAI | La tua chiave API xAI |
| Z.ai | La tua chiave API Z.ai |
| Z.ai Coding Plan | La tua chiave API Z.ai Coding Plan |
| ChatGPT · login | Accesso ChatGPT gestito dall'app-server Codex installato |

Scegli un modello nella dashboard, anche tra quelli disponibili specificamente per il tuo account. I controlli di connessione verificano il fornitore e il modello selezionato. I fornitori di API addebitano l'utilizzo al tuo account presso di loro.

La connessione Codex usa il suo protocollo ufficiale app-server. Hycli non legge né copia i token OAuth di Codex. L'inferenza viene eseguita in un thread temporaneo con le operazioni sui file e l'esecuzione di shell, browser, applicazioni e MCP disabilitate. La preparazione dei siti web avviene tramite gli strumenti di lettura controllati di Hycli.

<a id="use-hycli-with-your-ai"></a>
## Usa Hycli con la tua IA

**Agenti di programmazione → Visualizza impostazioni di connessione**

La connessione MCP generale comprende preparazione dei siti ed esecuzione delle azioni. Collega l’agente una volta affinché possa preparare gli strumenti mancanti, seguire i progressi e usare nuove azioni.

```sh
hycli mcp
```

Usa `--sites-only` per esporre solo le azioni installate. La connessione seguente è limitata a un sito.

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

Se `hycli` non è nel PATH dell’agente, usa il percorso dell’eseguibile mostrato nella dashboard.

Lo stesso flusso è disponibile dalla CLI. Sostituisci l’URL di esempio, `SITE` e `ACTION` con il tuo sito e i nomi restituiti da `describe`.

```sh
hycli prepare https://your-website.example --intent "Trovare i riferimenti salvati"
hycli request SITE "Add draft editing and publishing with full post content"
hycli describe
hycli describe SITE ACTION
hycli SITE ACTION --help
hycli run SITE ACTION --arg query="design systems"
hycli jobs show JOB_ID --watch
```

In MCP, passa URL e `intent` a `hycli_prepare`, poi segui il lavoro con `hycli_job` o `hycli_result`. `hycli_run` esegue nuove azioni anche prima che il client aggiorni l’elenco degli strumenti. Trova prima gli ID interni mediante le relative azioni di elenco o ricerca.

Le modifiche restituiscono una ricevuta da esaminare nella dashboard. Seguine l’esito senza ripetere la richiesta. Una lettura fallita o una risposta inattesa produce un errore anche nella CLI.

[Guida per agenti](../../agent/AGENTS.md) · [Skill Hycli](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## Accesso tramite browser

La preparazione del sito inizia esaminando il sistema operativo corrente, i browser in esecuzione e registrati e i profili disponibili. L’AI sceglie tra queste capacità per leggere la documentazione, visualizzare le pagine, importare una sessione del sito e verificare la connessione. I percorsi del browser e dei profili restano sul computer locale; la definizione generata del sito non dipende dai percorsi di installazione dello sviluppatore.

Cookie e dati di archiviazione dell’origine di Chrome, Chromium, Edge, Brave e Firefox possono essere importati quando accessibili. Solo la sessione del sito richiesto entra nel vault locale di Hycli. Hycli seleziona automaticamente un’unica identità verificata; i profili appartenenti alla stessa identità contano come una sola scelta di account. La scelta dell’account esistente viene mantenuta; identità verificate diverse richiedono una scelta.

In **Account → Collega un account**, **Trova il mio accesso** ripete la ricerca. Se non trova una sessione utilizzabile, **Apri la pagina di accesso** apre il sito nel browser predefinito. Hycli ricontrolla mentre la finestra è aperta e riprende la preparazione solo dopo aver verificato l’account selezionato. La sola apertura di una scheda non conferma l’accesso.

Le sessioni protette dal sistema operativo, vincolate al browser, private o in contenitori potrebbero richiedere l’estensione Hycli. Seleziona la scheda dell’estensione, crea un codice di connessione, apri l’estensione nel profilo in cui hai effettuato l’accesso, inserisci il codice e concedi l’accesso a quel sito. L’importazione nativa non aggira la protezione App-Bound di Windows né indebolisce il profilo del browser.

- **Chrome ed Edge:** decomprimi il pacchetto e usa **Carica estensione non pacchettizzata** nella pagina delle estensioni del browser.
- **Firefox:** usa il pacchetto Firefox e **Carica componente aggiuntivo temporaneo** in `about:debugging`. Le estensioni temporanee vengono rimosse al riavvio di Firefox. Questa versione non include la distribuzione firmata tramite gli store.
- **File di cookie:** come alternativa, puoi importare file di cookie JSON e Netscape. Importa il file direttamente nella dashboard; non incollarne il contenuto in una conversazione con un'IA.

Il browser trasferisce i dati della sessione direttamente nel vault locale delle credenziali. I modelli ricevono etichette degli account, nomi e struttura delle chiavi locali e risultati degli strumenti privati dei valori delle credenziali. Possono creare una procedura di connessione che fa riferimento a tali valori locali senza riceverli. Ambito, percorsi, scadenza e informazioni di partizionamento supportate dei cookie sono rispettati; le intestazioni di autenticazione restano limitate alla loro origine iniziale.

L'identità dell'account proviene da una risposta reale del sito sull'account corrente. Il nome di un profilo del browser non viene considerato un'identità verificata sul sito. Se l'account non è ancora stato identificato, Hycli lo segnala. Dopo il collegamento, la normale navigazione può fornire una risposta relativa all'account e le strutture delle richieste disponibili, senza esporre al modello di preparazione i valori delle richieste, le intestazioni o i corpi delle risposte.

Un sito può richiedere credenziali API separate e documentate oppure una funzione del browser non disponibile localmente. La preparazione registra i risultati effettivi della connessione e della lettura. Il recupero della pagina iniziale o una pagina di accesso restituita da un’API non rendono pronta l’integrazione. La [guida alla configurazione del browser](../../browser-companion/guide.html) spiega il flusso con l’estensione e i suoi limiti.

<a id="languages"></a>
## Lingue

La dashboard supporta le 20 lingue collegate sopra, incluso l'arabo con scrittura da destra a sinistra. La lingua iniziale è l'inglese. La tua scelta viene salvata su questo dispositivo e le descrizioni dell'IA possono essere preparate nella lingua selezionata.

<a id="local-data"></a>
## Dati locali e limiti delle richieste

Le definizioni dei siti web, i metadati degli account e le attività restano nella directory locale dei dati. Imposta `HYCLI_DATA_DIR` per scegliere un'altra posizione. Usa **Impostazioni** per vedere la posizione attiva e la protezione delle credenziali.

L'archivio delle credenziali usa una chiave di cifratura protetta dal portachiavi del sistema operativo, quando disponibile. In assenza di un portachiavi, segnala esplicitamente che l'archivio è protetto solo dai permessi dei file; le directory private e i file delle credenziali sono accessibili soltanto all'utente corrente del sistema operativo. Un archivio cifrato esistente non viene sostituito silenziosamente se non può essere sbloccato.

Le richieste vengono eseguite in sequenza e distanziate per sito e account, con un ulteriore limite complessivo per sito. Hycli rispetta `Retry-After`, limita le dimensioni delle risposte e i tentativi di lettura e si sospende in caso di errori di autenticazione o verifiche del sito. La preparazione dei siti non alterna account, falsifica impronte digitali o aggira verifiche per eludere una restrizione. Queste misure riducono il carico evitabile; non possono garantire che un sito non limiti mai un account.

Gli indirizzi di siti privati e locali sono disabilitati per impostazione predefinita. Per un sito attendibile ospitato autonomamente, avvia la dashboard o il server MCP indicando esplicitamente `--allow-local`.

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

I test usano siti locali e risposte dei fornitori sintetici. Coprono il vincolo delle approvazioni alle richieste e la prevenzione del riutilizzo, l'occultamento dei segreti, i reindirizzamenti, i limiti di frequenza, l'assenza di nuovi tentativi di scrittura, la protezione delle sessioni del browser e i contratti di risposta dei fornitori. Non usare mai un account reale per creare, modificare o eliminare contenuti solo per testare Hycli.

Le schermate e la GIF usano dati di esempio isolati. Consulta le [fonti delle immagini](../images/README.md).

## Licenza e uso previsto

Hycli è distribuito con licenza [Apache-2.0](../../LICENSE).

Hycli è pensato per creare e usare strumenti da riga di comando che semplificano l'utilizzo dei siti web. Usalo con siti e account ai quali sei autorizzato ad accedere.

Il software viene fornito così com'è, senza garanzie. Nei limiti consentiti dalla legge, gli autori e i collaboratori non sono responsabili di perdite o altri problemi derivanti dal suo utilizzo. Sei responsabile dell'uso che ne fai.

Nella misura consentita dalla legge, gli autori e i collaboratori non sono responsabili di blocchi, sospensioni o restrizioni degli account derivanti dall’uso di Hycli. Non usare Hycli per attività di hacking, accessi non autorizzati o attacchi.
