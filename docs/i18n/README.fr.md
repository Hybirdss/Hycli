<p align="center">
  <img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="Hycli — Des sites web prêts pour l’IA." width="100%" />
</p>

<p align="center">
  <strong>Transformez les sites web en outils utilisables par votre IA.</strong>
</p>

<p align="center">
  <a href="#get-started">Premiers pas</a> ·
  <a href="#what-your-ai-can-do">Ce que votre IA peut faire</a> ·
  <a href="#ai-connections">Connexions IA</a> ·
  <a href="#use-hycli-with-your-ai">Utiliser avec votre IA</a>
</p>

<details>
<summary>Lire dans votre langue · 20 langues</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

Ajoutez un site web. Hycli prépare des actions utilisables par votre IA et explique le rôle de chacune. Retrouvez vos sites web, vos connexions et vos résultats dans un tableau de bord local.

| Connecter | Préparer | Utiliser |
| --- | --- | --- |
| Ajoutez un site web et choisissez votre IA. | Suivez la progression et consultez les actions disponibles. | Exécutez une action ou confiez-la à votre assistant IA. |

<p align="center">
  <img src="../../docs/images/dashboard.png" alt="Tableau de bord Hycli avec les fiches des sites web, les actions disponibles et la progression des tâches." width="100%" />
</p>
<p align="center"><sub>Espace de travail d’exemple avec des outils et des comptes de démonstration.</sub></p>

<a id="get-started"></a>
## Premiers pas

**[Télécharger v0.1.0 · Linux x64](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)** · glibc ≥ 2.39 · [SHA-256](https://github.com/Hybirdss/Hycli/releases/download/v0.1.0/hycli-0.1.0-linux-x64.tar.gz.sha256)

Hycli est une application Rust dotée d'un tableau de bord web local. Depuis cette copie du dépôt, compilez le tableau de bord et l'exécutable :

```sh
node scripts/build.mjs
./dist/hycli dashboard
```

Le tableau de bord s'ouvre à l'adresse `http://127.0.0.1:4318`. Utilisez `hycli dashboard --no-open` pour afficher l'adresse sans ouvrir de navigateur, ou `--port 4320` pour choisir un autre port. Le moteur des sites web et le tableau de bord sont inclus dans le même exécutable.

1. Ouvrez **Connexions IA** et connectez un fournisseur.
2. Ajoutez l'adresse d'un site web et choisissez l'IA qui le préparera.
3. Examinez les actions disponibles. Exécutez-en une dans le tableau de bord ou ouvrez **Agents de programmation** pour copier une seule fois la configuration MCP.

Pour un site nécessitant une authentification, connectez son compte depuis **Comptes**. Plusieurs comptes peuvent être enregistrés pour un même site ; vous choisissez celui que ses outils utilisent.

<a id="what-your-ai-can-do"></a>
## Ce que votre IA peut faire

Hycli transforme les opérations prises en charge sur les sites web en outils nommés et typés. Les descriptions rédigées par l'IA expliquent ce que chaque action accomplit, les informations dont elle a besoin et ce qu'elle renvoie.

Trois agents IA indépendants prennent en charge les lectures et recherches, les flux de travail utiles et les informations sur les comptes. Le tableau de bord affiche les étapes terminées, l’état de chaque agent, le temps écoulé et des notes de travail en langage clair. Les opérations de la CLI et de MCP figurent dans le même historique d’activité.

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="Trois agents. Un petit oiseau très occupé." width="100%" />
</p>
<p align="center"><sub>Trois agents. Un petit oiseau très occupé.</sub></p>

La préparation s'appuie sur les informations réellement disponibles sur le site : documentation liée, schémas d'API, JavaScript publié et structures de requêtes observées par l'extension du navigateur. Elle peut effectuer des lectures limitées pour vérifier une opération. Elle ne crée pas de données de test, ne modifie pas de contenu, ne supprime pas d'objets, ne devine pas de longues listes de points de terminaison et ne réalise pas de fuzzing sur un serveur en production.

| Action | Comportement |
| --- | --- |
| Lire ou rechercher | S'exécute de manière autonome lorsque l'opération est étayée par des éléments concrets et classée comme lecture. |
| Créer, envoyer, modifier ou supprimer | Affiche le site web, le compte, l'action et les valeurs d'entrée résolues pour approbation par l'utilisateur. |
| Effet incertain | Nécessite une vérification avant l'envoi de la requête. |
| Authentification, défi ou limite de requêtes | Suspend les requêtes concernées et vous permet de vous reconnecter ou d'attendre. |

L'approbation porte sur une requête précise, expire après cinq minutes et ne peut être utilisée qu'une seule fois. Toute modification des entrées, du compte, des informations de connexion enregistrées ou de la définition de l'outil installé l'invalide. Les exécutions via la CLI, MCP et le tableau de bord respectent les mêmes règles. Les écritures ne sont jamais relancées automatiquement.

La prise en charge dépend du site. Une page sans documentation exploitable ni opérations observées peut nécessiter un navigateur connecté, un SiteSpec fourni ou une préparation supplémentaire. Hycli indique ce qu'il peut prendre en charge, sans prétendre que chaque site dispose d'une API prête à l'emploi.

### Fonctions prises en charge

Prise en charge d’OpenAPI JSON ou YAML, de REST, de GraphQL et des corps JSON ou de formulaire encodés pour les URL. Le HTML peut fournir des fiches, des liens et la page suivante à partir de sélecteurs observés ; les pages GET rendues par un Chromium local peuvent aussi être lues. Si une lecture échoue pendant la préparation, l’IA examine les éléments observés, corrige la définition et la vérifie à nouveau.

Les séquences arbitraires de clics, les envois multipart et les téléchargements binaires ne sont pas encore pris en charge. Les paquets ne contiennent aucun compte ni CLI générée auparavant.

[Contrat des opérations](../SITESPEC.md) · [Vérification des paquets](../RELEASING.md)

<a id="ai-connections"></a>
## Connexions IA

| Connexion | Authentification |
| --- | --- |
| ChatGPT · API key | Votre clé d'API OpenAI |
| Claude | Votre clé d'API Anthropic |
| xAI | Votre clé d'API xAI |
| Z.ai | Votre clé d'API Z.ai |
| Z.ai Coding Plan | Votre clé d'API Z.ai Coding Plan |
| ChatGPT · login | Connexion ChatGPT gérée par l'app-server Codex installé |

Choisissez un modèle dans le tableau de bord, y compris un modèle accessible spécifiquement à votre compte. Les vérifications de connexion contrôlent le fournisseur et le modèle sélectionné. Les fournisseurs d'API facturent l'utilisation sur votre compte chez eux.

La connexion Codex utilise son protocole officiel app-server. Hycli ne lit ni ne copie les jetons OAuth de Codex. L'inférence s'exécute dans un fil éphémère où les opérations sur les fichiers et l'exécution du shell, du navigateur, des applications et de MCP sont désactivées. Les outils de lecture contrôlés de Hycli assurent la préparation des sites web.

<a id="use-hycli-with-your-ai"></a>
## Utiliser Hycli avec votre IA

**Agents de développement → Voir les paramètres de connexion**

La connexion MCP générale permet de préparer les sites et d’exécuter leurs actions. Connectez votre agent une fois pour qu’il puisse créer les outils manquants, suivre la progression et utiliser les nouvelles actions.

```sh
hycli mcp
```

Utilisez `--sites-only` pour exposer uniquement les actions installées. La connexion suivante est limitée à un site.

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

Si `hycli` n’est pas dans le PATH de l’agent, utilisez le chemin de l’exécutable indiqué dans le tableau de bord.

Le même parcours est disponible en CLI. Remplacez l’URL d’exemple, `SITE` et `ACTION` par votre site et les noms renvoyés par `describe`.

```sh
hycli prepare https://your-website.example --intent "Retrouver les références enregistrées"
hycli request SITE "Add draft editing and publishing with full post content"
hycli describe
hycli describe SITE ACTION
hycli SITE ACTION --help
hycli run SITE ACTION --arg query="design systems"
hycli jobs show JOB_ID --watch
```

En MCP, transmettez l’URL et `intent` à `hycli_prepare`, puis suivez le travail avec `hycli_job` ou `hycli_result`. `hycli_run` peut exécuter de nouvelles actions avant même que le client actualise sa liste d’outils. Recherchez les identifiants internes avec les actions de liste ou de recherche associées.

Les modifications renvoient une demande à examiner dans le tableau de bord. Suivez son résultat sans répéter la demande. Une lecture échouée ou une réponse inattendue produit également un échec dans la CLI.

[Guide des agents](../../agent/AGENTS.md) · [Skill Hycli](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## Connexion via le navigateur

La préparation d’un site commence par l’examen du système d’exploitation actuel, des navigateurs en cours d’exécution ou enregistrés et des profils disponibles. L’IA choisit parmi ces possibilités pour lire la documentation, afficher les pages, importer une session du site et vérifier la connexion. Les chemins des navigateurs et des profils restent sur la machine locale ; la définition du site générée ne dépend pas des chemins d’installation du développeur.

Les cookies et le stockage d’origine de Chrome, Chromium, Edge, Brave et Firefox peuvent être importés lorsqu’ils sont accessibles. Seule la session du site demandé entre dans le coffre local de Hycli. Hycli sélectionne automatiquement une seule identité vérifiée ; les profils correspondant à cette identité comptent comme un seul choix de compte. Le compte déjà sélectionné est conservé ; des identités vérifiées différentes nécessitent un choix.

Dans **Comptes → Connecter un compte**, **Trouver ma session** relance la recherche. Si aucune session utilisable n’est trouvée, **Ouvrir la page de connexion** ouvre le site dans votre navigateur par défaut. Hycli vérifie à nouveau tant que la boîte de dialogue est ouverte et reprend la préparation uniquement après vérification du compte sélectionné. L’ouverture d’un onglet ne suffit pas à confirmer une connexion.

Les sessions protégées par le système d’exploitation, liées au navigateur, privées ou dans un conteneur peuvent nécessiter l’extension Hycli. Sélectionnez l’onglet de l’extension, créez un code de connexion, ouvrez l’extension dans le profil connecté, saisissez le code et autorisez l’accès à ce site. L’importation native ne contourne pas la protection App-Bound de Windows et n’affaiblit pas un profil de navigateur.

- **Chrome et Edge :** décompressez le paquet et utilisez **Charger l'extension non empaquetée** sur la page des extensions du navigateur.
- **Firefox :** utilisez le paquet Firefox et **Charger un module complémentaire temporaire** dans `about:debugging`. Les extensions temporaires sont supprimées au redémarrage de Firefox. Cette version n'inclut pas de distribution signée via les boutiques d'extensions.
- **Fichier de cookies :** l'importation de fichiers de cookies JSON et Netscape est disponible comme solution de repli. Importez le fichier directement dans le tableau de bord ; ne collez pas son contenu dans une conversation avec une IA.

Le navigateur transfère les données de session directement dans le coffre local des identifiants. Les modèles reçoivent les libellés des comptes, les noms et la structure des clés locales, ainsi que les résultats des outils dépourvus de valeurs d’identifiants. Ils peuvent créer une recette de connexion qui référence ces valeurs locales sans les recevoir. La portée, les chemins, l’expiration et les informations de partition prises en charge des cookies sont respectés, et les en-têtes d’authentification restent limités à leur origine initiale.

L'identité du compte provient d'une réponse réelle du site concernant le compte courant. Le nom d'un profil de navigateur n'est pas considéré comme une identité vérifiée sur le site. Si le compte n'a pas encore été identifié, Hycli le précise. Après la connexion, une navigation normale peut fournir une réponse de compte et les structures des requêtes disponibles sans exposer au modèle de préparation les valeurs des requêtes, leurs en-têtes ni les corps des réponses.

Un site peut nécessiter un identifiant d’API distinct et documenté ou une fonction du navigateur indisponible localement. La préparation consigne les résultats réels de connexion et de lecture. Charger la page d’accueil ou recevoir une page de connexion en réponse à une API ne signifie pas que l’intégration est prête. Le [guide de configuration du navigateur](../../browser-companion/guide.html) explique le fonctionnement de l’extension et ses limites.

<a id="languages"></a>
## Langues

Le tableau de bord prend en charge les 20 langues liées ci-dessus, dont l'arabe de droite à gauche. L'anglais est la langue initiale. Votre choix est enregistré sur cet appareil, et les descriptions de l'IA peuvent être préparées dans la langue sélectionnée.

<a id="local-data"></a>
## Données locales et limites de requêtes

Les définitions des sites web, les métadonnées des comptes et l'activité restent dans le répertoire local de données. Définissez `HYCLI_DATA_DIR` pour choisir un autre emplacement. Consultez **Paramètres** pour voir l'emplacement actif et la protection des identifiants.

Le coffre des identifiants utilise une clé de chiffrement protégée par le trousseau du système d'exploitation lorsqu'il est disponible. En l'absence de trousseau, il indique explicitement que le stockage n'est protégé que par les permissions des fichiers ; les répertoires privés et les fichiers d'identifiants sont réservés à l'utilisateur actuel du système. Un coffre chiffré existant n'est pas remplacé silencieusement lorsqu'il ne peut pas être déverrouillé.

Les requêtes sont exécutées en série et espacées par site et par compte, avec un quota supplémentaire à l'échelle du site. Hycli respecte `Retry-After`, limite la taille des réponses et les nouvelles tentatives de lecture, et se met en pause en cas d'échec d'authentification ou de défi du site. La préparation des sites web ne change pas de compte, ne falsifie pas les empreintes et ne contourne pas les défis pour éviter une restriction. Ces mesures réduisent la charge évitable ; elles ne peuvent garantir qu'un site ne restreindra jamais un compte.

Les adresses de sites privées et locales sont désactivées par défaut. Pour un site auto-hébergé de confiance, lancez le tableau de bord ou le serveur MCP avec l'option explicite `--allow-local`.

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

Les tests utilisent des sites locaux et des réponses de fournisseurs synthétiques. Ils couvrent la liaison des approbations aux requêtes et la prévention du rejeu, le masquage des secrets, les redirections, les limites de débit, l'absence de nouvelles tentatives d'écriture, la protection des sessions de navigateur et les contrats de réponse des fournisseurs. N'utilisez jamais un compte réel pour créer, modifier ou supprimer du contenu dans le seul but de tester Hycli.

Les captures et le GIF utilisent des données de démonstration isolées. Consultez les [sources visuelles](../images/README.md).

## Licence et utilisation prévue

Hycli est distribué sous licence [Apache-2.0](../../LICENSE).

Hycli est destiné à la création et à l'utilisation d'outils en ligne de commande facilitant l'usage des sites web. Utilisez-le avec des sites et des comptes auxquels vous êtes autorisé à accéder.

Le logiciel est fourni en l'état, sans garantie. Dans la mesure permise par la loi, ses auteurs et contributeurs ne sont pas responsables des pertes ou autres problèmes résultant de son utilisation. Vous êtes responsable de l'usage que vous en faites.

Dans la mesure permise par la loi, les auteurs et les contributeurs ne sont pas responsables des bannissements, suspensions ou restrictions de comptes résultant de l’utilisation de Hycli. N’utilisez pas Hycli pour pirater, accéder sans autorisation ou mener des attaques.
