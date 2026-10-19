<p align="center">
  <img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="Hycli — Sites prontos para a IA." width="100%" />
</p>

<p align="center">
  <strong>Transforme qualquer site em uma CLI tipada e em um servidor MCP para o seu agente de IA.</strong><br/>
  <sub>Leia, pesquise, crie, edite e automatize usando o seu login real do navegador, sem vazamento de cookies para LLMs e com resiliência antibot integrada.</sub>
</p>

<p align="center">
  <a href="https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0">Baixar (Windows · macOS · Linux)</a> ·
  <a href="https://github.com/Hybirdss/Hycli/issues">Obter ajuda no GitHub</a> ·
  <a href="#try-it-in-30-seconds">Teste em 30 segundos</a> ·
  <a href="#get-started">Primeiros passos</a> ·
  <a href="#what-your-ai-can-do">O que sua IA pode fazer</a> ·
  <a href="#common-concerns">Dúvidas comuns</a> ·
  <a href="#use-hycli-with-your-ai">Use com sua IA</a>
</p>

<details>
<summary>Leia no seu idioma · 20 idiomas</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

Informe ao Hycli a URL de qualquer site e o que você deseja fazer. Três agentes de IA inspecionam a documentação, os pacotes JS de frontend e o tráfego de rede real do navegador para construir uma CLI tipada e um conjunto de ferramentas MCP — cobrindo tanto leitura quanto escrita — sem nunca modificar sua conta durante a preparação.

<p align="center">
  <img src="../../docs/images/dashboard.png" alt="Painel do Hycli exibindo cartões de sites, ações disponíveis e o progresso das tarefas." width="100%" />
</p>
<p align="center"><sub>Um único painel local e CLI para seus sites, contas, permissões de escrita e resultados.</sub></p>

<a id="try-it-in-30-seconds"></a>
## Teste em 30 segundos

```sh
# 1. Aponte o Hycli para qualquer site e descreva seu objetivo (prepara leituras e escritas; zero alterações ao vivo)
hycli prepare https://your-website.example --intent "Pesquisar postagens, salvar rascunhos e publicar artigos"

# 2. Veja todas as ações de leitura, escrita e fluxos de trabalho em várias etapas preparados
hycli describe your-website

# 3. Execute ações de leitura imediatamente
hycli your-website search-posts --query "notas de lançamento"

# 4. Habilite a escrita uma única vez para esta CLI quando quiser criar, editar ou excluir
hycli writes your-website allow
hycli your-website create-draft --title "Olá do Hycli" --body "Automatizado direto do meu terminal."
```

Ou conecte o Hycli ao **Claude Code, Cursor, Codex ou Windsurf** uma única vez com `"command": "hycli", "args": ["mcp"]` e peça ao seu agente em linguagem natural:
> *"Use o Hycli para preparar `https://...`, mostre quais ações estão disponíveis e crie um novo rascunho de postagem."*

<a id="get-started"></a>
## Primeiros passos

O Hycli é distribuído como um único binário nativo contendo tanto o mecanismo de sites quanto o painel local (`http://127.0.0.1:4318`). Nem Rust nem Node.js são necessários para executar um pacote de lançamento.

### Baixar e executar (Windows, macOS, Linux)

Baixe o instalador nativo ou o pacote portátil para a sua plataforma em **[GitHub Releases (v0.1.0)](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)**:

| Plataforma | Arquitetura | Instalador nativo | Pacote portátil | Como iniciar |
| --- | --- | --- | --- | --- |
| **Windows 10/11** | x64 e ARM64 | `hycli-0.1.0-win32-*-setup.exe` | `hycli-0.1.0-win32-*.tar.gz` | Execute `*-setup.exe` (adiciona `hycli` ao `PATH` do usuário e ao Menu Iniciar sem exigir administrador) e abra o **Hycli** ou execute `hycli open` no PowerShell. |
| **macOS 13+** | Apple Silicon (`arm64`) e Intel (`x64`) | `hycli-0.1.0-darwin-*.dmg` | `hycli-0.1.0-darwin-*.tar.gz` | Abra o `.dmg`, arraste o **Hycli.app** para **Applications** e abra-o (vincula automaticamente `hycli` em `~/.local/bin`). |
| **Linux** | x64 e ARM64 | `hycli-0.1.0-linux-*.deb` | `hycli-0.1.0-linux-*.tar.gz` | Instale o `.deb` (`sudo apt install ./hycli-*.deb`) ou extraia o `.tar.gz` e execute `./install.sh`; depois execute `hycli open`. |

<details>
<summary><strong>Instalação rápida pelo terminal e compilação a partir do código-fonte</strong></summary>

**Pacote portátil para Linux / macOS:**

```sh
sha256sum -c hycli-0.1.0-*.tar.gz.sha256   # ou no macOS: shasum -a 256 -c
tar -xzf hycli-0.1.0-*.tar.gz
cd hycli-0.1.0-*
./install.sh
hycli open
```

**Pacote portátil para Windows PowerShell:**

```powershell
tar -xzf hycli-0.1.0-win32-x64.tar.gz
cd hycli-0.1.0-win32-x64
powershell -ExecutionPolicy Bypass -File .\install.ps1
hycli open
```

**Compilar a partir do código-fonte (Linux, macOS, Windows):**

Instale os [pré-requisitos de compilação](../BUILD.md) e execute:

```sh
node scripts/build.mjs
./dist/hycli open
```

Execute `node scripts/package-desktop.mjs` para gerar o instalador nativo `.exe`, `.dmg` ou `.deb` para o seu sistema operacional. Veja o guia de [compilação e empacotamento](../BUILD.md) e a [verificação de versão](../RELEASING.md).

</details>

### Conecte seu primeiro site

1. Execute `hycli open` (ou clique no ícone do aplicativo **Hycli**) para abrir o painel local em `http://127.0.0.1:4318`.
2. Abra **Conexões de IA** e conecte OpenAI, Anthropic Claude, xAI, Z.ai ou seu login local do ChatGPT Codex.
3. Para sites que exigem login, clique em **Contas → Encontrar meu login** para importar a sessão do navegador para o cofre local criptografado.
4. Digite a URL de um site e um objetivo opcional, ou copie a configuração MCP em **Agentes de programação** para o seu assistente de IA.

<a id="what-your-ai-can-do"></a>
## O que sua IA pode fazer

O Hycli transforma interfaces reais de sites em subcomandos CLI autodocumentados e ferramentas MCP:

| Interface do site | Capacidade preparada |
| --- | --- |
| REST e OpenAPI | Ações de leitura e escrita tipadas com base em esquemas JSON/YAML e documentação de API. |
| GraphQL | Documentos de consulta (`query`) e mutação (`mutation`) analisados com variáveis JSON tipadas. |
| Páginas HTML e renderizadas por JS | Registros estruturados, links, paginação e leituras GET renderizadas por meio de um motor Chromium local. |
| Formulários JSON e codificados em URL | Entradas documentadas, valores padrão e envios de formulários vinculados à conta (`POST`, `PUT`, `PATCH`, `DELETE`). |

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="Três trabalhadores. Um passarinho muito ocupado." width="100%" />
</p>
<p align="center"><sub>Três trabalhadores. Um passarinho muito ocupado.</sub></p>

| Tipo de ação | Comportamento de execução |
| --- | --- |
| **Leitura ou pesquisa** | Executa imediatamente quando baseada em evidências e classificada como leitura segura. |
| **Criar, enviar, editar ou excluir** | Descoberta e sintetizada na CLI sem nunca ser executada durante a preparação. Desativada por padrão (`writes: false`). Seu agente de IA pergunta a você **uma única vez por CLI** se deseja habilitar a escrita (`hycli writes SITE allow` ou MCP `hycli_writes`). Depois de habilitadas, as ações de escrita são executadas diretamente quando você solicita. |
| **Efeito incerto** | Retida para revisão antes do envio. |
| **Desafio de autenticação ou limite de taxa** | Alterna a estratégia de transporte (HTTP → `window.fetch` na página → Chromium furtivo) ou pausa com segurança. |

Consulte a [referência de operações](../SITESPEC.md) para ver os formatos de requisição suportados e as regras de extração.

<a id="common-concerns"></a>
## Dúvidas comuns e como o Hycli as resolve

Conectar um agente de IA a sites reais e contas autenticadas levanta questões sérias. Veja como o Hycli resolve cada uma delas por arquitetura:

### 1. "A IA vai publicar, editar ou excluir dados na minha conta por acidente?"
- **Zero alterações ao vivo durante a criação da CLI:** Ao preparar ou verificar um site (`hycli prepare`, `hycli request`, `hycli recon`, `hycli check`), o Hycli descobre e compila operações de leitura e escrita na especificação da CLI, mas **nunca executa requisições de escrita**. Mesmo mutações disfarçadas (rotas `GET` com caminhos de alteração como `/delete`, corpos `mutation` em GraphQL ou formulários HTML `<form method="post">`) são bloqueadas durante a verificação.
- **Confirmação de escrita uma única vez por CLI:** Toda nova CLI de site começa com `writes: false`. Quando um agente de IA precisa executar uma ação de escrita em uma CLI pela primeira vez, ele pergunta a você **uma única vez para aquela CLI**: *"Permitir ações de escrita para `<site>`?"* Somente após a sua confirmação ele executa `hycli writes <site> allow` (ou MCP `hycli_writes`), habilitando a execução direta de escritas para essa CLI. Você pode revogar a permissão a qualquer momento com `hycli writes <site> deny` ou pelo botão **Permitir alterações** no painel.
- **Sem novas tentativas automáticas para escrita:** Requisições de escrita que falham ou são interrompidas nunca são reenviadas automaticamente. Se a conexão cair após o envio de uma mutação, o Hycli informa `action_outcome_unknown` para que você verifique o site em vez de duplicar um pedido ou publicação.

### 2. "O Hycli envia meus cookies, tokens de login ou chaves de API para o modelo de IA?"
- **Segredos nunca entram nos prompts do LLM:** Cookies do navegador, tokens de `localStorage` / `sessionStorage` e chaves de API são armazenados estritamente no seu cofre local (criptografado via chaveiro do sistema no Gerenciador de Credenciais do Windows, Chaves do macOS ou Linux Secret Service, ou restrito a permissões `0600` exclusivas do usuário).
- **Receitas simbólicas de conexão (`ConnectionRecipe`):** Os modelos de IA veem apenas rótulos de conta, nomes de chaves de armazenamento e esquemas de resposta mascarados. O modelo constrói uma `ConnectionRecipe` simbólica (fazendo referência ao nome de um cookie ou token), e o Hycli injeta o segredo real localmente no momento da execução, removendo segredos de todas as saídas e registros de log.
- **Importação somente leitura restrita ao site:** A importação de uma sessão do navegador lê apenas os cookies e o armazenamento do domínio solicitado a partir de um snapshot temporário somente leitura. Isso nunca enfraquece a segurança do navegador nem exporta dados de outros sites.

### 3. "O uso do Hycli vai acionar CAPTCHAs ou banir minha conta?"
- **Sem fuzzing ou varredura por força bruta:** O Hycli nunca adivinha listas de endpoints nem faz fuzzing em servidores reais. Ele aprende com a documentação vinculada, esquemas OpenAPI/GraphQL, pacotes JS estáticos e formatos de rede observados no navegador.
- **Tráfego serializado e com ritmo controlado:** As requisições são serializadas e têm taxa limitada por site e por conta, respeitando `Retry-After` e limitando as novas tentativas de leitura.
- **Disjuntor de estratégia e execução nativa no navegador:** Quando o HTTP simples encontra um erro 403/429 ou um desafio antibot (Cloudflare, Akamai Bot Manager), o Hycli nunca repete em loop a requisição bloqueada. Seu disjuntor alterna imediatamente o transporte — executando `window.fetch(..., { credentials: 'include' })` dentro do contexto do seu navegador autenticado ou usando um motor Chromium furtivo local — ou pausa de forma limpa para proteger sua conta.

### 4. "Um redirecionamento pode vazar minha sessão ou uma especificação compartilhada pode expor caminhos do meu computador?"
- **Vinculação estrita à origem:** Cabeçalhos de autenticação e cookies ficam travados na origem de destino e são removidos imediatamente em redirecionamentos entre origens (cross-origin). Faixas de IP privadas e de loopback são bloqueadas por padrão, a menos que `--allow-local` seja passado explicitamente.
- **Especificações portáteis:** Os caminhos locais do executável e do perfil do navegador permanecem no estado local da máquina e nunca são gravados nos arquivos portáteis `SiteSpec`.

### 5. "A IA vai desperdiçar tokens tentando novamente quando meu login expirar?"
- **Verificação de integridade determinística sem IA (`hycli check`):** `hycli check` (e MCP `hycli_check`) reproduz cadeias de leitura verificadas sem invocar um LLM e separa claramente `signed_out` (reconectar sua conta) de `site_changed` (reparar especificação), `blocked` e `unreachable`.

<a id="ai-connections"></a>
## Conexões de IA

| Provedor | Autenticação |
| --- | --- |
| ChatGPT · chave de API | Chave de API da OpenAI |
| Claude | Chave de API da Anthropic |
| xAI | Chave de API da xAI |
| Z.ai / Z.ai Coding Plan | Chave de API da Z.ai |
| ChatGPT · login | Login local do ChatGPT por meio do protocolo oficial Codex app-server |

A conexão de login do Codex é executada em uma thread efêmera com execução de arquivos, shell, navegador e MCP externo desativada; o Hycli nunca lê nem copia tokens OAuth do Codex.

<a id="use-hycli-with-your-ai"></a>
## Use o Hycli com sua IA

Adicione o Hycli ao seu cliente MCP (**Agentes de programação → Ver configurações de conexão** mostra o caminho exato para o seu sistema operacional):

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

Use `hycli mcp --sites-only` para expor apenas as ferramentas de sites instalados, ou adicione `--only-site SITE` para restringir a um único site. O servidor MCP fornece `hycli_prepare`, `hycli_writes` (habilita escritas uma vez por CLI após pedir sua confirmação), `hycli_run`, `hycli_check`, `hycli_result` e `hycli_activity` — e nunca expõe uma ferramenta para ler credenciais.

Pelo terminal, todos os fluxos de trabalho funcionam da mesma maneira:

```sh
hycli prepare https://your-website.example --intent "Encontrar referências salvas"
hycli request SITE "Adicionar edição e publicação de rascunhos com o conteúdo completo da postagem"
hycli describe SITE
hycli SITE ACTION --help
hycli writes SITE allow
hycli SITE ACTION --query "design systems"
hycli check SITE
```

[Guia do agente](../../agent/AGENTS.md) · [Habilidade do Hycli](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## Login do navegador

O Hycli descobre perfis locais do Chrome, Chromium, Edge, Brave, Vivaldi, Arc, Whale e Firefox (além de binários locais de Chromium furtivo, quando instalados) no Windows, macOS e Linux, importando apenas a sessão do site solicitado para o cofre local. Perfis que compartilham a mesma conta verificada de um site são desduplicados automaticamente.

Se uma sessão estiver bloqueada pela criptografia App-Bound do Windows, contêineres privados ou um navegador não importado, use **Contas → Conectar uma conta** para entrar pelo navegador, conecte a [extensão complementar do navegador](../../browser-companion/guide.html) ou importe um arquivo de cookies JSON/Netscape no painel (nunca cole cookies em um chat de IA).

<a id="languages"></a>
## Idiomas

O painel e as descrições de ações geradas por IA oferecem suporte aos 20 idiomas listados no topo deste README, incluindo árabe da direita para a esquerda (RTL).

<a id="local-data"></a>
## Dados locais e limites de requisição

As definições de sites, credenciais criptografadas e registros de atividade ficam no diretório de dados do seu sistema operacional (`%LOCALAPPDATA%\hycli` no Windows, `~/Library/Application Support/hycli` no macOS, `~/.local/share/hycli` no Linux ou `HYCLI_DATA_DIR`). Inspecione a proteção ativa do cofre a qualquer momento em **Configurações**.

<a id="contributing"></a>
## Desenvolvimento e verificação

```sh
npm --prefix dashboard ci
npm --prefix dashboard run check
npm --prefix dashboard run build
cargo test --all-targets --locked
cargo build --locked --bin hycli --example dashboard_fixture
node scripts/test-e2e.mjs --full
```

Todos os testes são executados contra sites locais sintéticos e fixtures — nunca teste operações de escrita em uma conta real de terceiros. Consulte a [procedência dos recursos visuais](../images/README.md).

## Licença e uso pretendido

O Hycli é licenciado sob a [Apache-2.0](../../LICENSE). Use-o apenas com sites e contas aos quais você tem autorização de acesso. Fornecido "no estado em que se encontra" (as is), sem garantias; os autores e colaboradores não se responsabilizam por quaisquer perdas, restrições de conta ou banimentos resultantes do seu uso.

## Star History

<a href="https://www.star-history.com/#Hybirdss/Hycli&Date">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date&theme=dark" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
 </picture>
</a>
