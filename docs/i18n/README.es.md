<p align="center">
  <img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="Hycli — Sitios web listos para la IA." width="100%" />
</p>

<p align="center">
  <strong>Convierte cualquier sitio web en una CLI tipada y un servidor MCP para tu agente de IA.</strong><br/>
  <sub>Lee, busca, crea, edita y automatiza usando tu inicio de sesión real del navegador, sin filtrar cookies a los LLM y con resistencia integrada contra bloqueos antibot.</sub>
</p>

<p align="center">
  <a href="https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0">Descargar (Windows · macOS · Linux)</a> ·
  <a href="https://github.com/Hybirdss/Hycli/issues">Obtener ayuda en GitHub</a> ·
  <a href="#try-it-in-30-seconds">Pruébalo en 30 segundos</a> ·
  <a href="#get-started">Primeros pasos</a> ·
  <a href="#what-your-ai-can-do">Qué puede hacer tu IA</a> ·
  <a href="#common-concerns">Preguntas frecuentes</a> ·
  <a href="#use-hycli-with-your-ai">Úsalo con tu IA</a>
</p>

<details>
<summary>Leer en tu idioma · 20 idiomas</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

Indica a Hycli la URL de cualquier sitio web y lo que quieres hacer. Tres trabajadores de IA inspeccionan su documentación, los paquetes JS del frontend y el tráfico de red real del navegador para construir una CLI tipada y un conjunto de herramientas MCP —que cubren tanto lecturas como escrituras— sin modificar jamás tu cuenta durante la preparación.

<p align="center">
  <img src="../../docs/images/dashboard.png" alt="Panel de Hycli mostrando tarjetas de sitios web, acciones disponibles y el progreso de las tareas." width="100%" />
</p>
<p align="center"><sub>Un único panel local y CLI para tus sitios web, cuentas, permisos de escritura y resultados.</sub></p>

<a id="try-it-in-30-seconds"></a>
## Pruébalo en 30 segundos

```sh
# 1. Apunta Hycli a cualquier sitio web y describe tu objetivo (prepara lecturas y escrituras; cero modificaciones en vivo)
hycli prepare https://your-website.example --intent "Buscar publicaciones, guardar borradores y publicar artículos"

# 2. Consulta todas las lecturas, escrituras y flujos de trabajo de varios pasos preparados
hycli describe your-website

# 3. Ejecuta acciones de lectura de inmediato
hycli your-website search-posts --query "notas de la versión"

# 4. Habilita las escrituras una sola vez para esta CLI cuando quieras crear, editar o eliminar
hycli writes your-website allow
hycli your-website create-draft --title "Hola desde Hycli" --body "Automatizado desde mi terminal."
```

O conecta Hycli a **Claude Code, Cursor, Codex o Windsurf** una sola vez con `"command": "hycli", "args": ["mcp"]` y pídele a tu agente en lenguaje natural:
> *"Usa Hycli para preparar `https://...`, muéstrame qué acciones están disponibles y redacta una nueva publicación."*

<a id="get-started"></a>
## Primeros pasos

Hycli se distribuye como un único binario nativo que incluye tanto el motor de sitios web como el panel local (`http://127.0.0.1:4318`). No se necesita ni Rust ni Node.js para ejecutar un paquete de lanzamiento.

### Descargar y ejecutar (Windows, macOS, Linux)

Descarga el instalador nativo o el archivo portable para tu plataforma desde **[GitHub Releases (v0.1.0)](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)**:

| Plataforma | Arquitectura | Instalador nativo | Archivo portable | Cómo empezar |
| --- | --- | --- | --- | --- |
| **Windows 10/11** | x64 y ARM64 | `hycli-0.1.0-win32-*-setup.exe` | `hycli-0.1.0-win32-*.tar.gz` | Ejecuta `*-setup.exe` (añade `hycli` al `PATH` de tu usuario y al Menú Inicio sin requerir permisos de administrador), luego abre **Hycli** o ejecuta `hycli open` en PowerShell. |
| **macOS 13+** | Apple Silicon (`arm64`) e Intel (`x64`) | `hycli-0.1.0-darwin-*.dmg` | `hycli-0.1.0-darwin-*.tar.gz` | Abre el archivo `.dmg`, arrastra **Hycli.app** a **Applications** y ábrelo (enlaza automáticamente `hycli` en `~/.local/bin`). |
| **Linux** | x64 y ARM64 | `hycli-0.1.0-linux-*.deb` | `hycli-0.1.0-linux-*.tar.gz` | Instala el `.deb` (`sudo apt install ./hycli-*.deb`) o extrae el `.tar.gz` y ejecuta `./install.sh`; después ejecuta `hycli open`. |

<details>
<summary><strong>Instalación rápida por terminal y compilación desde el código fuente</strong></summary>

**Archivo portable para Linux / macOS:**

```sh
sha256sum -c hycli-0.1.0-*.tar.gz.sha256   # o en macOS: shasum -a 256 -c
tar -xzf hycli-0.1.0-*.tar.gz
cd hycli-0.1.0-*
./install.sh
hycli open
```

**Archivo portable para Windows PowerShell:**

```powershell
tar -xzf hycli-0.1.0-win32-x64.tar.gz
cd hycli-0.1.0-win32-x64
powershell -ExecutionPolicy Bypass -File .\install.ps1
hycli open
```

**Compilar desde el código fuente (Linux, macOS, Windows):**

Instala los [requisitos de compilación](../BUILD.md) y ejecuta:

```sh
node scripts/build.mjs
./dist/hycli open
```

Ejecuta `node scripts/package-desktop.mjs` para generar el instalador nativo `.exe`, `.dmg` o `.deb` para tu sistema operativo. Consulta la [guía de compilación y empaquetado](../BUILD.md) y la [verificación de versiones](../RELEASING.md).

</details>

### Conecta tu primer sitio web

1. Ejecuta `hycli open` (o haz clic en el icono de la aplicación **Hycli**) para abrir el panel local en `http://127.0.0.1:4318`.
2. Abre **Conexiones de IA** y conecta OpenAI, Anthropic Claude, xAI, Z.ai o tu inicio de sesión local de ChatGPT Codex.
3. Para sitios que requieren sesión iniciada, haz clic en **Cuentas → Buscar mi inicio de sesión** para importar la sesión de tu navegador a la bóveda local cifrada.
4. Introduce la URL de un sitio web y un objetivo opcional, o copia la configuración MCP desde **Agentes de programación** en tu asistente de IA.

<a id="what-your-ai-can-do"></a>
## Qué puede hacer tu IA

Hycli convierte interfaces web reales en subcomandos CLI autodocumentados y herramientas MCP:

| Interfaz del sitio web | Capacidad preparada |
| --- | --- |
| REST y OpenAPI | Acciones de lectura y escritura tipadas basadas en esquemas JSON/YAML y documentación de API. |
| GraphQL | Documentos de consulta (`query`) y mutación (`mutation`) analizados con variables JSON tipadas. |
| Páginas HTML y renderizadas con JS | Registros estructurados, enlaces, paginación y lecturas GET renderizadas mediante un motor Chromium local. |
| Formularios JSON y codificados en URL | Entradas documentadas, valores predeterminados y envíos de formularios asociados a la cuenta (`POST`, `PUT`, `PATCH`, `DELETE`). |

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="Tres trabajadores. Un pajarito muy ocupado." width="100%" />
</p>
<p align="center"><sub>Tres trabajadores. Un pajarito muy ocupado.</sub></p>

| Tipo de acción | Comportamiento de ejecución |
| --- | --- |
| **Lectura o búsqueda** | Se ejecuta de inmediato cuando está respaldada por evidencia y clasificada como lectura segura. |
| **Crear, enviar, editar o eliminar** | Se descubre y sintetiza en la CLI sin ejecutarse jamás durante la preparación. Desactivada de forma predeterminada (`writes: false`). Tu agente de IA te pregunta **una sola vez por CLI** si deseas permitir escrituras (`hycli writes SITE allow` o MCP `hycli_writes`). Una vez habilitadas, las acciones de escritura se ejecutan directamente cuando las pides. |
| **Efecto poco claro** | Se retiene para revisión antes de enviarse. |
| **Desafío de autenticación o límite de tasa** | Cambia de estrategia de transporte (HTTP → `window.fetch` dentro de la página → Chromium sigiloso) o se pausa de forma segura. |

Consulta la [referencia de operaciones](../SITESPEC.md) para ver los formatos de solicitud admitidos y las reglas de extracción.

<a id="common-concerns"></a>
## Preguntas frecuentes y cómo las resuelve Hycli

Conectar un agente de IA a sitios web reales y cuentas con sesión iniciada plantea dudas importantes. Así es como Hycli aborda cada una por diseño:

### 1. "¿La IA publicará, editará o eliminará datos de mi cuenta por accidente?"
- **Cero modificaciones en vivo durante la creación de la CLI:** Mientras prepara o verifica un sitio web (`hycli prepare`, `hycli request`, `hycli recon`, `hycli check`), Hycli descubre y compila tanto operaciones de lectura como de escritura en la especificación de la CLI, pero **nunca ejecuta solicitudes de escritura**. Incluso las mutaciones disfrazadas (rutas `GET` con caminos de modificación como `/delete`, cuerpos `mutation` de GraphQL o formularios HTML `<form method="post">`) se bloquean durante la verificación.
- **Confirmación de escritura una sola vez por CLI:** Cada nueva CLI de un sitio web comienza con `writes: false`. Cuando un agente de IA necesita ejecutar una acción de escritura en una CLI por primera vez, te pregunta **una sola vez para esa CLI**: *"¿Permitir acciones de escritura para `<site>`?"* Solo después de tu confirmación ejecuta `hycli writes <site> allow` (o MCP `hycli_writes`), habilitando la ejecución directa de escrituras para esa CLI. Puedes revocar el permiso en cualquier momento con `hycli writes <site> deny` o con el interruptor **Permitir cambios** del panel.
- **Sin reintentos automáticos en escrituras:** Las solicitudes de escritura fallidas o interrumpidas nunca se reintentan automáticamente. Si la conexión se corta después de enviar una modificación, Hycli informa `action_outcome_unknown` para que puedas comprobar el sitio web en lugar de duplicar un pedido o una publicación.

### 2. "¿Hycli envía mis cookies, tokens de sesión o claves de API al modelo de IA?"
- **Los secretos nunca entran en los prompts del LLM:** Las cookies del navegador, los tokens de `localStorage` / `sessionStorage` y las claves de API se almacenan estrictamente en tu bóveda local (cifrada mediante el llavero del sistema operativo en el Administrador de credenciales de Windows, el Llavero de macOS o Linux Secret Service, o restringida con permisos `0600` exclusivos de tu usuario).
- **Recetas de conexión simbólicas (`ConnectionRecipe`):** Los modelos de IA solo ven etiquetas de cuenta, nombres de claves de almacenamiento y esquemas de respuesta redactados. El modelo construye una `ConnectionRecipe` simbólica (que hace referencia al nombre de una cookie o token), y Hycli inyecta el secreto real localmente en el momento de la ejecución mientras elimina cualquier secreto de todas las salidas y registros.
- **Importación de solo lectura limitada al sitio:** Al importar una sesión del navegador, solo se leen las cookies y el almacenamiento del dominio solicitado a partir de una instantánea temporal de solo lectura. Nunca debilita la seguridad del navegador ni exporta otros sitios web.

### 3. "¿El uso de Hycli activará CAPTCHAs o hará que suspendan mi cuenta?"
- **Sin fuzzing ni escaneo por fuerza bruta:** Hycli nunca adivina listas de endpoints ni realiza fuzzing contra servidores en vivo. Aprende de la documentación enlazada, esquemas OpenAPI/GraphQL, paquetes JS estáticos y estructuras de red reales observadas en el navegador.
- **Tráfico serializado y con ritmo controlado:** Las solicitudes se serializan y limitan en velocidad por sitio web y por cuenta, respetando `Retry-After` y acotando los reintentos de lectura.
- **Disyuntor de estrategia y ejecución en navegador nativo:** Cuando una solicitud HTTP simple recibe un 403/429 o un desafío antibot (Cloudflare, Akamai Bot Manager), Hycli nunca repite en bucle la solicitud bloqueada. Su disyuntor cambia inmediatamente de transporte —ejecutando `window.fetch(..., { credentials: 'include' })` dentro del contexto de tu navegador autenticado o usando un motor Chromium sigiloso local— o se pausa limpiamente para proteger tu cuenta.

### 4. "¿Puede una redirección filtrar mi sesión o una especificación compartida exponer las rutas de mi equipo?"
- **Vinculación estricta al origen:** Los encabezados de autenticación y las cookies están bloqueados al origen de destino y se eliminan de inmediato en redirecciones entre orígenes (cross-origin). Los rangos de IP privadas y de loopback se bloquean de forma predeterminada a menos que se pase explícitamente `--allow-local`.
- **Especificaciones portables:** Las rutas locales del ejecutable y del perfil del navegador permanecen en el estado local de la máquina y nunca se escriben en archivos `SiteSpec` portables.

### 5. "¿La IA desperdiciará tokens reintentando cuando caduque mi sesión?"
- **Diagnóstico determinista sin IA (`hycli check`):** `hycli check` (y MCP `hycli_check`) reproduce cadenas de lectura verificadas sin invocar ningún LLM y distingue con precisión entre `signed_out` (volver a conectar la cuenta), `site_changed` (reparar la especificación), `blocked` y `unreachable`.

<a id="ai-connections"></a>
## Conexiones de IA

| Proveedor | Autenticación |
| --- | --- |
| ChatGPT · clave de API | Clave de API de OpenAI |
| Claude | Clave de API de Anthropic |
| xAI | Clave de API de xAI |
| Z.ai / Z.ai Coding Plan | Clave de API de Z.ai |
| ChatGPT · inicio de sesión | Inicio de sesión local de ChatGPT mediante el protocolo oficial Codex app-server |

La conexión mediante inicio de sesión de Codex se ejecuta en un hilo efímero con la ejecución de archivos, shell, navegador y MCP externos desactivada; Hycli nunca lee ni copia tokens OAuth de Codex.

<a id="use-hycli-with-your-ai"></a>
## Usa Hycli con tu IA

Añade Hycli a tu cliente MCP (**Agentes de programación → Ver configuración de conexión** muestra la ruta exacta para tu sistema operativo):

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

Usa `hycli mcp --sites-only` para exponer únicamente las herramientas de sitios web instalados, o añade `--only-site SITE` para limitarlo a un solo sitio. El servidor MCP proporciona `hycli_prepare`, `hycli_writes` (habilita las escrituras una vez por CLI tras preguntarte), `hycli_run`, `hycli_check`, `hycli_result` y `hycli_activity`, y nunca expone una herramienta para leer credenciales.

Desde la terminal, todos los flujos de trabajo funcionan exactamente igual:

```sh
hycli prepare https://your-website.example --intent "Encontrar referencias guardadas"
hycli request SITE "Añadir edición y publicación de borradores con el contenido completo"
hycli describe SITE
hycli SITE ACTION --help
hycli writes SITE allow
hycli SITE ACTION --query "design systems"
hycli check SITE
```

[Guía para agentes](../../agent/AGENTS.md) · [Habilidad de Hycli](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## Inicio de sesión del navegador

Hycli detecta perfiles locales de Chrome, Chromium, Edge, Brave, Vivaldi, Arc, Whale y Firefox (además de binarios locales de Chromium sigiloso cuando están instalados) en Windows, macOS y Linux, importando únicamente la sesión del sitio web solicitado a la bóveda local. Los perfiles que comparten la misma cuenta verificada de un sitio web se deduplican automáticamente.

Si una sesión está protegida por el cifrado App-Bound de Windows, contenedores privados o un navegador no importado, usa **Cuentas → Conectar una cuenta** para iniciar sesión desde tu navegador, conecta la [extensión complementaria del navegador](../../browser-companion/guide.html) o importa un archivo de cookies JSON/Netscape en el panel (nunca pegues cookies en un chat de IA).

<a id="languages"></a>
## Idiomas

El panel y las descripciones de acciones generadas por IA admiten los 20 idiomas enlazados en la parte superior de este README, incluido el árabe de derecha a izquierda (RTL).

<a id="local-data"></a>
## Datos locales y límites de solicitudes

Las definiciones de sitios web, las credenciales cifradas y los registros de actividad se guardan en el directorio de datos de tu sistema operativo (`%LOCALAPPDATA%\hycli` en Windows, `~/Library/Application Support/hycli` en macOS, `~/.local/share/hycli` en Linux o `HYCLI_DATA_DIR`). Puedes inspeccionar el estado de protección de la bóveda en cualquier momento desde **Configuración**.

<a id="contributing"></a>
## Desarrollo y verificación

```sh
npm --prefix dashboard ci
npm --prefix dashboard run check
npm --prefix dashboard run build
cargo test --all-targets --locked
cargo build --locked --bin hycli --example dashboard_fixture
node scripts/test-e2e.mjs --full
```

Todas las pruebas se ejecutan contra sitios web locales sintéticos y fixtures: nunca pruebes operaciones de escritura contra una cuenta real de terceros. Consulta la [procedencia de recursos visuales](../images/README.md).

## Licencia y uso previsto

Hycli se distribuye bajo la licencia [Apache-2.0](../../LICENSE). Úsalo únicamente con sitios web y cuentas a los que tengas autorización para acceder. Se proporciona "tal cual", sin garantías; los autores y colaboradores no se hacen responsables de pérdidas, restricciones de cuenta o bloqueos derivados de su uso.

## Star History

<a href="https://www.star-history.com/#Hybirdss/Hycli&Date">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date&theme=dark" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
 </picture>
</a>
