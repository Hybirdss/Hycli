<p align="center">
  <img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="Hycli — Sitios web listos para la IA." width="100%" />
</p>

<p align="center">
  <strong>Convierte los sitios web en herramientas que tu IA puede usar.</strong>
</p>

<p align="center">
  <a href="#get-started">Primeros pasos</a> ·
  <a href="#what-your-ai-can-do">Qué puede hacer tu IA</a> ·
  <a href="#ai-connections">Conexiones de IA</a> ·
  <a href="#use-hycli-with-your-ai">Usar con tu IA</a>
</p>

<details>
<summary>Lee en tu idioma · 20 idiomas</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

Añade un sitio web. Hycli prepara acciones que tu IA puede usar y explica qué hace cada una. Reúne tus sitios web, sesiones y resultados en un panel local.

| Conectar | Preparar | Usar |
| --- | --- | --- |
| Añade un sitio web y elige tu IA. | Sigue el progreso y consulta las acciones disponibles. | Ejecuta una acción o ponla a disposición de tu asistente de IA. |

<p align="center">
  <img src="../../docs/images/dashboard.png" alt="Panel de Hycli con tarjetas de sitios web, acciones disponibles y progreso de tareas." width="100%" />
</p>
<p align="center"><sub>Espacio de trabajo de ejemplo con herramientas y cuentas de muestra.</sub></p>

<a id="get-started"></a>
Introduce un dominio y, opcionalmente, un objetivo a su lado. La IA primero entiende el sitio y elige procesos útiles; después comprueba los requisitos, los datos y los resultados. Si faltan pasos, el sitio sigue marcado para revisión.

## Primeros pasos

**[Descargar v0.1.0 · Linux x64](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)** · glibc ≥ 2.39 · [SHA-256](https://github.com/Hybirdss/Hycli/releases/download/v0.1.0/hycli-0.1.0-linux-x64.tar.gz.sha256)

Hycli es una aplicación en Rust con un panel web local. Desde esta copia del repositorio, compila el panel y el ejecutable:

```sh
node scripts/build.mjs
./dist/hycli dashboard
```

El panel se abre en `http://127.0.0.1:4318`. Usa `hycli dashboard --no-open` para mostrar la dirección sin abrir un navegador, o `--port 4320` para elegir otro puerto. El motor de sitios web y el panel se incluyen en el mismo ejecutable.

1. Abre **Conexiones de IA** y conecta un proveedor.
2. Añade la dirección de un sitio web y elige la IA que lo preparará.
3. Revisa las acciones disponibles. Ejecuta una desde el panel o abre **Agentes de programación** para copiar una sola vez la configuración MCP.

Para un sitio web que requiere iniciar sesión, conecta su cuenta desde **Cuentas**. Un sitio puede tener varias cuentas guardadas; tú eliges cuál utilizan sus herramientas.

<a id="what-your-ai-can-do"></a>
## Qué puede hacer tu IA

Hycli convierte las operaciones compatibles de un sitio web en herramientas con nombre y tipos definidos. Las descripciones escritas por la IA explican qué hace cada acción, qué información necesita y qué devuelve.

Tres agentes de IA independientes se encargan de las lecturas y búsquedas, los flujos de trabajo útiles y la información sobre las cuentas. El panel muestra las etapas completadas, el estado de cada agente, el tiempo transcurrido y notas de trabajo fáciles de entender. El trabajo de la CLI y MCP aparece en el mismo historial de actividad.

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="Tres trabajadores. Un pajarito muy ocupado." width="100%" />
</p>
<p align="center"><sub>Tres trabajadores. Un pajarito muy ocupado.</sub></p>

La preparación se basa en información realmente disponible en el sitio: documentación enlazada, esquemas de API, JavaScript publicado y estructuras de solicitudes observadas por la extensión del navegador. Puede realizar lecturas acotadas para comprobar una operación. No crea registros de prueba, edita contenido, elimina objetos, adivina largas listas de endpoints ni somete un servidor activo a pruebas de fuzzing.

| Acción | Comportamiento |
| --- | --- |
| Leer o buscar | Se ejecuta de forma autónoma cuando hay evidencias que respaldan la operación y está clasificada como lectura. |
| Crear, enviar, editar o eliminar | Muestra el sitio web, la cuenta, la acción y los valores de entrada resueltos para que el usuario los apruebe. |
| Efecto incierto | Requiere una revisión antes de enviar la solicitud. |
| Autenticación, desafío o límite de solicitudes | Pausa las solicitudes afectadas y permite volver a conectar o esperar. |

La aprobación se aplica a una solicitud exacta, caduca a los cinco minutos y solo puede utilizarse una vez. Cambiar las entradas, la cuenta, los datos de acceso guardados o la definición de la herramienta instalada la invalida. La ejecución desde la CLI, MCP y el panel respeta el mismo límite. Las escrituras nunca se reintentan automáticamente.

La compatibilidad depende del sitio web. Una página sin documentación utilizable ni operaciones observadas puede necesitar un navegador con una sesión iniciada, un SiteSpec proporcionado o preparación adicional. Hycli indica qué puede admitir, sin afirmar que todos los sitios tengan una API lista para usar.

### Capacidades compatibles

Admite OpenAPI en JSON o YAML, REST, GraphQL y cuerpos JSON o de formulario codificados para URL. Extrae registros, enlaces y la siguiente página del HTML mediante selectores observados; también puede leer páginas GET renderizadas con Chromium local. Si una lectura falla durante la preparación, la IA revisa la evidencia, corrige la definición y vuelve a comprobarla.

No se admiten todavía secuencias arbitrarias de clics, cargas multipart ni descargas binarias. Los paquetes no incluyen cuentas ni CLI generadas previamente.

[Contrato de operaciones](../SITESPEC.md) · [Verificación de paquetes](../RELEASING.md)

<a id="ai-connections"></a>
## Conexiones de IA

| Conexión | Inicio de sesión |
| --- | --- |
| ChatGPT · API key | Tu clave de API de OpenAI |
| Claude | Tu clave de API de Anthropic |
| xAI | Tu clave de API de xAI |
| Z.ai | Tu clave de API de Z.ai |
| Z.ai Coding Plan | Tu clave de API de Z.ai Coding Plan |
| ChatGPT · login | Inicio de sesión de ChatGPT gestionado por el app-server de Codex instalado |

Elige un modelo en el panel, incluidos los disponibles específicamente para tu cuenta. Las comprobaciones de conexión verifican el proveedor y el modelo seleccionado. Los proveedores de API facturan el uso a tu cuenta con el proveedor.

La conexión de Codex utiliza su protocolo oficial app-server. Hycli no lee ni copia los tokens OAuth de Codex. La inferencia se ejecuta en un hilo efímero, con la ejecución de archivos, shell, navegador, aplicaciones y MCP desactivada. Las herramientas de lectura controladas de Hycli se encargan de preparar los sitios web.

<a id="use-hycli-with-your-ai"></a>
## Usar Hycli con tu IA

**Agentes de programación → Ver ajustes de conexión**

La conexión MCP general permite preparar sitios y ejecutar acciones. Conecta tu agente una vez para que pueda preparar herramientas que faltan, seguir el progreso y usar nuevas acciones.

```sh
hycli mcp
```

Usa `--sites-only` para exponer solo acciones instaladas. Esta conexión se limita a un sitio.

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

Si `hycli` no está en el PATH del agente, usa la ruta del ejecutable que muestra el panel.

El mismo flujo está disponible en la CLI. Sustituye la URL de ejemplo, `SITE` y `ACTION` por tu sitio y los nombres que devuelve `describe`.

```sh
hycli prepare https://your-website.example --intent "Buscar referencias guardadas"
hycli request SITE "Add draft editing and publishing with full post content"
hycli describe
hycli describe SITE ACTION
hycli SITE ACTION --help
hycli run SITE ACTION --arg query="design systems"
hycli jobs show JOB_ID --watch
```

En MCP, pasa la URL y `intent` a `hycli_prepare` y sigue el trabajo con `hycli_job` o `hycli_result`. `hycli_run` ejecuta acciones nuevas aunque el cliente aún no haya actualizado su lista de herramientas. Resuelve los identificadores internos mediante acciones relacionadas de búsqueda o listado.

Los cambios devuelven un comprobante para revisar en el panel. Sigue su resultado sin repetir la solicitud. Una lectura fallida o una respuesta inesperada también produce un estado de error en la CLI.

[Guía para agentes](../../agent/AGENTS.md) · [Skill de Hycli](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## Inicio de sesión en el navegador

La preparación del sitio comienza examinando el sistema operativo actual, los navegadores en ejecución y registrados, y los perfiles disponibles. La IA elige entre esas capacidades para leer documentación, mostrar páginas, importar una sesión del sitio y verificar la conexión. Las rutas del navegador y de los perfiles permanecen en el equipo local; la definición generada del sitio no depende de las rutas de instalación del desarrollador.

Se pueden importar las cookies y el almacenamiento de origen de Chrome, Chromium, Edge, Brave y Firefox cuando son accesibles. Solo la sesión del sitio solicitado entra en la bóveda local de Hycli. Hycli selecciona automáticamente una única identidad verificada; los perfiles de esa misma identidad cuentan como una sola opción de cuenta. Se conserva la selección de cuenta existente; las identidades verificadas distintas requieren una elección.

En **Cuentas → Conectar una cuenta**, **Buscar mi sesión iniciada** repite la búsqueda. Si no encuentra una sesión válida, **Abrir la página de inicio de sesión** abre el sitio en tu navegador predeterminado. Hycli vuelve a comprobar la sesión mientras el cuadro de diálogo está abierto y reanuda la preparación solo cuando se verifica la cuenta seleccionada. Abrir una pestaña por sí solo no se considera un inicio de sesión correcto.

Las sesiones protegidas por el sistema operativo, vinculadas al navegador, privadas o en contenedores pueden requerir la extensión de Hycli. Selecciona la pestaña de la extensión, crea un código de conexión, abre la extensión en el perfil con la sesión iniciada, introduce el código y concede acceso a ese sitio web. La importación nativa no elude la protección App-Bound de Windows ni debilita el perfil del navegador.

- **Chrome y Edge:** descomprime el paquete y utiliza **Cargar descomprimida** en la página de extensiones del navegador.
- **Firefox:** utiliza el paquete para Firefox y **Cargar complemento temporal** en `about:debugging`. Las extensiones temporales se eliminan al reiniciar Firefox. Esta versión no incluye distribución firmada en las tiendas.
- **Archivo de cookies:** como alternativa, puedes importar archivos de cookies JSON y Netscape. Importa el archivo directamente en el panel; no pegues su contenido en una conversación con una IA.

El navegador transfiere los datos de sesión directamente a la bóveda local de credenciales. Los modelos reciben etiquetas de cuentas, nombres y estructuras de claves locales, y resultados de herramientas sin valores de credenciales. Pueden crear una receta de conexión que haga referencia a esos valores locales sin recibirlos. Se respetan el ámbito, las rutas, la caducidad y la información de partición compatible de las cookies, y las cabeceras de autenticación quedan restringidas a su origen original.

La identidad de la cuenta procede de una respuesta real del sitio sobre la cuenta actual. El nombre de un perfil del navegador no se considera una identidad verificada del sitio web. Si la cuenta aún no se ha identificado, Hycli lo indica. Tras conectar, la navegación habitual puede aportar una respuesta de cuenta y las estructuras de las solicitudes disponibles sin exponer al modelo de preparación los valores de las solicitudes, las cabeceras ni los cuerpos de las respuestas.

Un sitio puede requerir una credencial de API independiente y documentada o una capacidad del navegador que no esté disponible localmente. La preparación registra los resultados reales de conexión y lectura. Obtener la página principal o recibir una página de inicio de sesión desde una API no significa que la integración esté lista. La [guía de configuración del navegador](../../browser-companion/guide.html) explica el proceso con la extensión y sus limitaciones.

<a id="languages"></a>
## Idiomas

El panel admite los 20 idiomas enlazados arriba, incluido el árabe de derecha a izquierda. El idioma inicial es el inglés. Tu elección se guarda en este dispositivo y las descripciones de la IA pueden prepararse en el idioma seleccionado.

<a id="local-data"></a>
## Datos locales y límites de solicitudes

Las definiciones de sitios web, los metadatos de cuentas y la actividad permanecen en el directorio local de datos. Configura `HYCLI_DATA_DIR` para elegir otra ubicación. En **Configuración** puedes consultar la ubicación activa y la protección de las credenciales.

La bóveda de credenciales utiliza una clave de cifrado respaldada por el llavero del sistema operativo cuando está disponible. Si no hay llavero, indica explícitamente que el almacenamiento solo está protegido por permisos de archivos; los directorios privados y los archivos de credenciales están restringidos al usuario actual del sistema operativo. Una bóveda cifrada existente no se reemplaza silenciosamente si no puede desbloquearse.

Las solicitudes se ejecutan en serie y se espacian por sitio y cuenta, con un límite adicional para todo el sitio. Hycli respeta `Retry-After`, limita el tamaño de las respuestas y los reintentos de lectura, y se pausa ante fallos de autenticación o desafíos del sitio. La preparación de sitios web no alterna cuentas, falsifica huellas digitales ni elude desafíos para evitar una restricción. Estas medidas reducen la carga evitable, pero no pueden garantizar que un sitio nunca restrinja una cuenta.

Las direcciones de sitios privados y locales están desactivadas de forma predeterminada. Para un sitio de confianza alojado por ti, inicia el panel o el servidor MCP indicando explícitamente `--allow-local`.

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

Las pruebas utilizan sitios locales y respuestas de proveedores sintéticos. Cubren la vinculación de aprobaciones y la prevención de reutilización, la ocultación de secretos, las redirecciones, los límites de frecuencia, la ausencia de reintentos de escritura, la protección de sesiones del navegador y los contratos de respuesta de los proveedores. Nunca utilices una cuenta real para crear, modificar o eliminar contenido solo para probar Hycli.

Las capturas y el GIF utilizan datos de ejemplo aislados. Consulta las [fuentes visuales](../images/README.md).

## Licencia y uso previsto

Hycli se distribuye bajo la licencia [Apache-2.0](../../LICENSE).

Hycli está pensado para crear y utilizar herramientas de línea de comandos que faciliten el uso de sitios web. Úsalo con sitios web y cuentas a los que tengas autorización para acceder.

El software se proporciona tal cual, sin garantía. En la medida permitida por la ley, sus autores y colaboradores no serán responsables de pérdidas u otros problemas derivados de su uso. Tú eres responsable del uso que hagas de él.

En la medida permitida por la ley, los autores y colaboradores no son responsables de los bloqueos, suspensiones o restricciones de cuentas derivados del uso de Hycli. No uses Hycli para hackear, acceder sin autorización o realizar ataques.
