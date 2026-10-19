<p align="center">
  <img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="Hycli — Les sites web, prêts pour l'IA." width="100%" />
</p>

<p align="center">
  <strong>Transformez n'importe quel site web en une CLI typée et un serveur MCP pour votre agent d'IA.</strong><br/>
  <sub>Lisez, recherchez, créez, modifiez et automatisez en utilisant votre vraie session de navigateur, sans fuite de cookies vers les LLM et avec une résilience anti-bot intégrée.</sub>
</p>

<p align="center">
  <a href="https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0">Télécharger (Windows · macOS · Linux)</a> ·
  <a href="https://github.com/Hybirdss/Hycli/issues">Obtenir de l’aide sur GitHub</a> ·
  <a href="#try-it-in-30-seconds">Essayer en 30 secondes</a> ·
  <a href="#get-started">Démarrer</a> ·
  <a href="#what-your-ai-can-do">Ce que votre IA peut faire</a> ·
  <a href="#common-concerns">Questions fréquentes</a> ·
  <a href="#use-hycli-with-your-ai">Utiliser avec votre IA</a>
</p>

<details>
<summary>Lire dans votre langue · 20 langues</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

Donnez à Hycli l'URL de n'importe quel site web et ce que vous souhaitez accomplir. Trois travailleurs d'IA inspectent sa documentation, ses bundles JS frontend et le trafic réseau réel du navigateur pour construire une CLI typée et un jeu d'outils MCP — couvrant aussi bien la lecture que l'écriture — sans jamais modifier votre compte pendant la préparation.

<p align="center">
  <img src="../../docs/images/dashboard.png" alt="Tableau de bord Hycli affichant les cartes de sites web, les actions disponibles et la progression des tâches." width="100%" />
</p>
<p align="center"><sub>Un seul tableau de bord local et une seule CLI pour vos sites web, vos comptes, vos autorisations d'écriture et vos résultats.</sub></p>

<a id="try-it-in-30-seconds"></a>
## Essayer en 30 secondes

```sh
# 1. Indiquez un site web à Hycli et décrivez votre objectif (prépare lectures et écritures ; aucune modification en direct)
hycli prepare https://your-website.example --intent "Rechercher des articles, enregistrer des brouillons et publier des billets"

# 2. Consultez toutes les lectures, écritures et workflows en plusieurs étapes préparés
hycli describe your-website

# 3. Exécutez immédiatement les actions de lecture
hycli your-website search-posts --query "notes de version"

# 4. Autorisez l'écriture une seule fois pour cette CLI lorsque vous êtes prêt à créer, modifier ou supprimer
hycli writes your-website allow
hycli your-website create-draft --title "Bonjour depuis Hycli" --body "Automatisé depuis mon terminal."
```

Ou connectez Hycli à **Claude Code, Cursor, Codex ou Windsurf** une seule fois avec `"command": "hycli", "args": ["mcp"]` et demandez à votre agent en langage naturel :
> *« Utilise Hycli pour préparer `https://...`, montre-moi les actions disponibles et rédige un nouveau brouillon. »*

<a id="get-started"></a>
## Démarrer

Hycli est distribué sous la forme d'un binaire natif unique contenant à la fois le moteur web et le tableau de bord local (`http://127.0.0.1:4318`). Ni Rust ni Node.js ne sont nécessaires pour exécuter une version publiée.

### Télécharger et lancer (Windows, macOS, Linux)

Téléchargez l'installateur natif ou l'archive portable correspondant à votre plateforme depuis **[GitHub Releases (v0.1.0)](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)** :

| Plateforme | Architecture | Installateur natif | Archive portable | Comment démarrer |
| --- | --- | --- | --- | --- |
| **Windows 10/11** | x64 et ARM64 | `hycli-0.1.0-win32-*-setup.exe` | `hycli-0.1.0-win32-*.tar.gz` | Exécutez `*-setup.exe` (ajoute `hycli` au `PATH` utilisateur et au menu Démarrer sans droits administrateur), puis lancez **Hycli** ou exécutez `hycli open` dans PowerShell. |
| **macOS 13+** | Apple Silicon (`arm64`) et Intel (`x64`) | `hycli-0.1.0-darwin-*.dmg` | `hycli-0.1.0-darwin-*.tar.gz` | Ouvrez le `.dmg`, faites glisser **Hycli.app** dans **Applications**, puis ouvrez-le (crée automatiquement le lien `hycli` dans `~/.local/bin`). |
| **Linux** | x64 et ARM64 | `hycli-0.1.0-linux-*.deb` | `hycli-0.1.0-linux-*.tar.gz` | Installez le `.deb` (`sudo apt install ./hycli-*.deb`) ou extrayez le `.tar.gz` et lancez `./install.sh`, puis exécutez `hycli open`. |

<details>
<summary><strong>Installation rapide depuis le terminal et compilation depuis les sources</strong></summary>

**Archive portable Linux / macOS :**

```sh
sha256sum -c hycli-0.1.0-*.tar.gz.sha256   # ou sur macOS : shasum -a 256 -c
tar -xzf hycli-0.1.0-*.tar.gz
cd hycli-0.1.0-*
./install.sh
hycli open
```

**Archive portable Windows PowerShell :**

```powershell
tar -xzf hycli-0.1.0-win32-x64.tar.gz
cd hycli-0.1.0-win32-x64
powershell -ExecutionPolicy Bypass -File .\install.ps1
hycli open
```

**Compiler depuis les sources (Linux, macOS, Windows) :**

Installez les [prérequis de compilation](../BUILD.md) et exécutez :

```sh
node scripts/build.mjs
./dist/hycli open
```

Exécutez `node scripts/package-desktop.mjs` pour produire l'installateur natif `.exe`, `.dmg` ou `.deb` pour votre système d'exploitation. Consultez [la compilation et l'empaquetage](../BUILD.md) ainsi que [la vérification des versions](../RELEASING.md).

</details>

### Connecter votre premier site web

1. Exécutez `hycli open` (ou cliquez sur l'icône de l'application **Hycli**) pour ouvrir le tableau de bord local sur `http://127.0.0.1:4318`.
2. Ouvrez **Connexions IA** et connectez OpenAI, Anthropic Claude, xAI, Z.ai ou votre session locale ChatGPT Codex.
3. Pour les sites nécessitant une connexion, cliquez sur **Comptes → Trouver ma session** afin d'importer la session de votre navigateur dans le coffre-fort local chiffré.
4. Saisissez l'URL d'un site web et un objectif facultatif, ou copiez la configuration MCP depuis **Agents de programmation** dans votre assistant d'IA.

<a id="what-your-ai-can-do"></a>
## Ce que votre IA peut faire

Hycli transforme les interfaces réelles des sites web en sous-commandes CLI auto-documentées et en outils MCP :

| Interface du site web | Capacité préparée |
| --- | --- |
| REST et OpenAPI | Actions de lecture et d'écriture typées basées sur les schémas JSON/YAML et la documentation API. |
| GraphQL | Documents de requête (`query`) et de mutation (`mutation`) analysés avec des variables JSON typées. |
| Pages HTML et rendues en JS | Enregistrements structurés, liens, pagination et lectures GET rendues via un moteur Chromium local. |
| Formulaires JSON et encodés en URL | Entrées documentées, valeurs par défaut et envois de formulaires liés au compte (`POST`, `PUT`, `PATCH`, `DELETE`). |

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="Trois travailleurs. Un tout petit oiseau très occupé." width="100%" />
</p>
<p align="center"><sub>Trois travailleurs. Un tout petit oiseau très occupé.</sub></p>

| Type d'action | Comportement d'exécution |
| --- | --- |
| **Lecture ou recherche** | S'exécute immédiatement lorsqu'elle s'appuie sur des preuves vérifiées et est classée comme lecture sûre. |
| **Créer, envoyer, modifier ou supprimer** | Découverte et synthétisée dans la CLI sans jamais être exécutée pendant la préparation. Désactivée par défaut (`writes: false`). Votre agent d'IA vous demande **une seule fois par CLI** si vous souhaitez activer l'écriture (`hycli writes SITE allow` ou MCP `hycli_writes`). Une fois activées, les actions d'écriture s'exécutent directement à votre demande. |
| **Effet incertain** | Mise en attente pour vérification avant tout envoi. |
| **Défi d'authentification ou limite de débit** | Change de stratégie de transport (HTTP → `window.fetch` dans la page → Chromium furtif) ou se met proprement en pause. |

Consultez la [référence des opérations](../SITESPEC.md) pour connaître les formats de requêtes pris en charge et les règles d'extraction.

<a id="common-concerns"></a>
## Questions fréquentes et comment Hycli y répond

Connecter un agent d'IA à de vrais sites web et à des comptes connectés soulève des questions légitimes. Voici comment Hycli les résout dès sa conception :

### 1. « L'IA risque-t-elle de publier, modifier ou supprimer des données sur mon compte par accident ? »
- **Aucune modification en direct lors de la création de la CLI :** Lors de la préparation ou de la vérification d'un site web (`hycli prepare`, `hycli request`, `hycli recon`, `hycli check`), Hycli découvre et compile les opérations de lecture comme d'écriture dans la spécification CLI, mais **n'exécute jamais de requêtes d'écriture**. Même les mutations déguisées (routes `GET` avec des chemins modificateurs comme `/delete`, corps `mutation` GraphQL ou formulaires HTML `<form method="post">`) sont bloquées pendant la vérification.
- **Confirmation d'écriture unique par CLI :** Chaque nouvelle CLI de site web démarre avec `writes: false`. Lorsqu'un agent d'IA doit exécuter une action d'écriture sur une CLI pour la première fois, il vous demande **une seule fois pour cette CLI** : *« Autoriser les actions d'écriture pour `<site>` ? »* Ce n'est qu'après votre confirmation qu'il exécute `hycli writes <site> allow` (ou MCP `hycli_writes`), activant l'exécution directe des écritures pour cette CLI. Vous pouvez révoquer cette autorisation à tout moment avec `hycli writes <site> deny` ou via le bouton **Autoriser les modifications** du tableau de bord.
- **Aucun nouvel essai automatique pour les écritures :** Les requêtes d'écriture ayant échoué ou ayant été interrompues ne sont jamais relancées automatiquement. Si la connexion est coupée après l'envoi d'une mutation, Hycli renvoie `action_outcome_unknown` afin que vous puissiez vérifier l'état sur le site au lieu de dupliquer une commande ou une publication.

### 2. « Hycli envoie-t-il mes cookies, mes jetons de session ou mes clés API au modèle d'IA ? »
- **Les secrets n'entrent jamais dans les prompts du LLM :** Les cookies du navigateur, les jetons `localStorage` / `sessionStorage` et les clés API sont stockés exclusivement dans votre coffre-fort local (chiffré via le trousseau du système : Gestionnaire d'informations d'identification Windows, Trousseau macOS ou Linux Secret Service, ou restreint aux permissions utilisateur `0600`).
- **Recettes de connexion symboliques (`ConnectionRecipe`) :** Les modèles d'IA ne voient que les libellés de compte, les noms de clés de stockage et les schémas de réponse masqués. Le modèle construit une `ConnectionRecipe` symbolique (faisant référence au nom d'un cookie ou d'un jeton), et Hycli injecte le secret réel localement au moment de l'exécution tout en nettoyant les secrets de toutes les sorties et de tous les journaux.
- **Importation en lecture seule limitée au site :** L'importation d'une session de navigateur ne lit que les cookies et le stockage du domaine cible à partir d'un instantané temporaire en lecture seule. Elle n'affaiblit jamais la sécurité du navigateur et n'exporte aucun autre site web.

### 3. « Le scraping va-t-il déclencher des CAPTCHA ou faire bannir mon compte ? »
- **Pas de fuzzing ni de balayage par force brute :** Hycli ne devine jamais de listes d'endpoints et ne fait jamais de fuzzing sur des serveurs en production. Il apprend à partir de la documentation liée, des schémas OpenAPI/GraphQL, des bundles JS statiques et des requêtes réseau réellement observées dans le navigateur.
- **Trafic sérialisé et cadencé :** Les requêtes sont sérialisées et limitées en débit par site web et par compte, en respectant l'en-tête `Retry-After` et en bornant les tentatives de lecture.
- **Disjoncteur de stratégie et exécution dans le navigateur natif :** Lorsqu'une requête HTTP classique rencontre une erreur 403/429 ou un défi anti-bot (Cloudflare, Akamai Bot Manager), Hycli ne répète jamais en boucle la requête bloquée. Son disjoncteur bascule immédiatement de transport — en exécutant `window.fetch(..., { credentials: 'include' })` dans le contexte de votre navigateur authentifié ou via un moteur Chromium furtif local — ou se met proprement en pause pour protéger votre compte.

### 4. « Une redirection peut-elle divulguer ma session, ou une spécification partagée exposer les chemins de mon ordinateur ? »
- **Liaison stricte à l'origine :** Les en-têtes d'authentification et les cookies sont verrouillés sur l'origine cible et immédiatement supprimés lors de redirections vers une autre origine (cross-origin). Les plages d'adresses IP privées et de bouclage (loopback) sont bloquées par défaut sauf si `--allow-local` est explicitement passé.
- **Spécifications portables :** Les chemins locaux de l'exécutable et des profils du navigateur restent dans l'état local de la machine et ne sont jamais écrits dans les fichiers `SiteSpec` portables.

### 5. « L'IA va-t-elle gaspiller des tokens en réessayant lorsque ma session expire ? »
- **Vérification d'état déterministe sans IA (`hycli check`) :** `hycli check` (et MCP `hycli_check`) rejoue les chaînes de lecture vérifiées sans invoquer de LLM et distingue clairement `signed_out` (reconnecter votre compte) de `site_changed` (réparer la spécification), `blocked` et `unreachable`.

<a id="ai-connections"></a>
## Connexions IA

| Fournisseur | Authentification |
| --- | --- |
| ChatGPT · clé API | Clé API OpenAI |
| Claude | Clé API Anthropic |
| xAI | Clé API xAI |
| Z.ai / Z.ai Coding Plan | Clé API Z.ai |
| ChatGPT · connexion | Connexion ChatGPT locale via le protocole officiel Codex app-server |

La connexion via Codex s'exécute dans un fil éphémère où l'accès aux fichiers, au shell, au navigateur et aux serveurs MCP externes est désactivé ; Hycli ne lit ni ne copie jamais les jetons OAuth de Codex.

<a id="use-hycli-with-your-ai"></a>
## Utiliser Hycli avec votre IA

Ajoutez Hycli à votre client MCP (**Agents de programmation → Voir les paramètres de connexion** affiche le chemin exact pour votre système d'exploitation) :

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

Utilisez `hycli mcp --sites-only` pour n'exposer que les outils des sites web installés, ou ajoutez `--only-site SITE` pour restreindre l'accès à un seul site. Le serveur MCP fournit `hycli_prepare`, `hycli_writes` (active les écritures une fois par CLI après vous avoir demandé confirmation), `hycli_run`, `hycli_check`, `hycli_result` et `hycli_activity` — et n'expose jamais d'outil permettant de lire les identifiants.

Depuis le terminal, chaque workflow fonctionne exactement de la même manière :

```sh
hycli prepare https://your-website.example --intent "Trouver les références enregistrées"
hycli request SITE "Ajouter l'édition et la publication de brouillons avec le contenu complet"
hycli describe SITE
hycli SITE ACTION --help
hycli writes SITE allow
hycli SITE ACTION --query "design systems"
hycli check SITE
```

[Guide pour les agents](../../agent/AGENTS.md) · [Compétence Hycli](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## Connexion au navigateur

Hycli détecte les profils locaux de Chrome, Chromium, Edge, Brave, Vivaldi, Arc, Whale et Firefox (ainsi que les binaires locaux de Chromium furtif lorsqu'ils sont installés) sous Windows, macOS et Linux, et n'importe que la session du site web demandé dans le coffre-fort local. Les profils partageant le même compte vérifié sur un site web sont automatiquement dédupliqués.

Si une session est protégée par le chiffrement App-Bound de Windows, des conteneurs privés ou un navigateur non importé, utilisez **Comptes → Connecter un compte** pour vous connecter via votre navigateur, associez l'[extension compagnon de navigateur](../../browser-companion/guide.html) ou importez un fichier de cookies JSON/Netscape dans le tableau de bord (ne collez jamais de cookies dans une conversation avec une IA).

<a id="languages"></a>
## Langues

Le tableau de bord et les descriptions d'actions générées par l'IA prennent en charge les 20 langues listées en haut de ce README, y compris l'arabe de droite à gauche (RTL).

<a id="local-data"></a>
## Données locales et limites de requêtes

Les définitions de sites web, les identifiants chiffrés et les journaux d'activité sont conservés dans le répertoire de données de votre système d'exploitation (`%LOCALAPPDATA%\hycli` sous Windows, `~/Library/Application Support/hycli` sous macOS, `~/.local/share/hycli` sous Linux ou `HYCLI_DATA_DIR`). Vous pouvez vérifier l'état de protection du coffre-fort à tout moment dans **Paramètres**.

<a id="contributing"></a>
## Développement et vérification

```sh
npm --prefix dashboard ci
npm --prefix dashboard run check
npm --prefix dashboard run build
cargo test --all-targets --locked
cargo build --locked --bin hycli --example dashboard_fixture
node scripts/test-e2e.mjs --full
```

Tous les tests s'exécutent sur des sites web locaux synthétiques et des fixtures — ne testez jamais d'opérations d'écriture sur un véritable compte tiers. Consultez la [provenance des ressources visuelles](../images/README.md).

## Licence et utilisation prévue

Hycli est distribué sous licence [Apache-2.0](../../LICENSE). Utilisez-le uniquement avec des sites web et des comptes auxquels vous êtes autorisé à accéder. Fourni « en l'état » sans garantie ; les auteurs et contributeurs déclinent toute responsabilité en cas de pertes, de restrictions de compte ou de bannissements résultant de son utilisation.

## Star History

<a href="https://www.star-history.com/#Hybirdss/Hycli&Date">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date&theme=dark" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
 </picture>
</a>
