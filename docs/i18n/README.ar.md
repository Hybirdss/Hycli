<p align="center">
  <img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="Hycli — مواقع ويب جاهزة للذكاء الاصطناعي." width="100%" />
</p>

<p align="center" dir="rtl">
  <strong>حوّل أي موقع ويب إلى أداة CLI وخادم MCP مُهيكلين لوكيل الذكاء الاصطناعي الخاص بك.</strong><br/>
  <sub>اقرأ وابحث وأنشئ وعدّل وأتمت — باستخدام تسجيل دخولك الفعلي في المتصفح، مع عدم تسريب أي ملفات تعريف ارتباط إلى نماذج اللغة ومقاومة مدمجة لحظر البوتات.</sub>
</p>

<p align="center" dir="rtl">
  <a href="https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0">تنزيل (Windows · macOS · Linux)</a> ·
  <a href="https://github.com/Hybirdss/Hycli/issues">احصل على المساعدة عبر GitHub</a> ·
  <a href="#try-it-in-30-seconds">جرّبه في 30 ثانية</a> ·
  <a href="#get-started">البدء</a> ·
  <a href="#what-your-ai-can-do">ما يمكن لذكائك الاصطناعي فعله</a> ·
  <a href="#common-concerns">مخاوف شائعة</a> ·
  <a href="#use-hycli-with-your-ai">الاستخدام مع الذكاء الاصطناعي</a>
</p>

<details>
<summary>اقرأ بلغتك · 20 لغة</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

أعطِ Hycli رابط أي موقع ويب وما تريد القيام به. يفحص ثلاثة عمال ذكاء اصطناعي وثائق الموقع وحزم JavaScript للواجهة الأمامية وحركة شبكة المتصفح الفعلية لبناء مجموعة أدوات CLI وMCP مُهيكلة — تشمل القراءة والكتابة معًا — دون إجراء أي تعديل فعلي على حسابك أثناء الإعداد.

<p align="center">
  <img src="../../docs/images/dashboard.png" alt="لوحة تحكم Hycli تعرض بطاقات المواقع والعمليات المتاحة وتقدم المهام." width="100%" />
</p>
<p align="center" dir="rtl"><sub>لوحة تحكم محلية واحدة وواجهة CLI لمواقعك وحساباتك وأذونات الكتابة ووكلاء الذكاء الاصطناعي.</sub></p>

<a id="try-it-in-30-seconds"></a>
## جرّبه في 30 ثانية

```sh
# 1. وجّه Hycli إلى أي موقع وصف هدفك (يجهّز القراءة والكتابة؛ صفر تعديلات فعلية أثناء التجهيز)
hycli prepare https://your-website.example --intent "البحث في المنشورات وحفظ المسودات ونشر المقالات"

# 2. استعرض جميع عمليات القراءة والكتابة ومسارات العمل متعددة الخطوات المُجهّزة
hycli describe your-website

# 3. نفّذ عمليات القراءة والبحث فورًا
hycli your-website search-posts --query "ملاحظات الإصدار"

# 4. فعّل الكتابة مرة واحدة لهذا الـ CLI عندما تكون مستعدًا للإنشاء أو التعديل أو الحذف
hycli writes your-website allow
hycli your-website create-draft --title "مرحبًا من Hycli" --body "تمت الأتمتة مباشرةً من الطرفية."
```

أو اربط Hycli مع **Claude Code أو Cursor أو Codex أو Windsurf** مرة واحدة باستخدام `"command": "hycli", "args": ["mcp"]` واطلب من وكيلك بلغة طبيعية:
> *"استخدم Hycli لتجهيز `https://...`، واعرض لي الإجراءات المتاحة، واكتب مسودة منشور جديد."*

<a id="get-started"></a>
## البدء

يأتي Hycli كملف تنفيذي أصلي واحد يضم محرك المواقع ولوحة التحكم المحلية (`http://127.0.0.1:4318`). لا حاجة إلى تثبيت Rust أو Node.js لتشغيل حزمة الإصدار.

### التنزيل والتشغيل (Windows وmacOS وLinux)

نزّل المثبّت الأصلي أو الأرشيف المحمول لمنصتك من **[إصدارات GitHub (v0.1.0)](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)**:

| المنصة | المعمارية | المثبّت الأصلي | الأرشيف المحمول | كيفية البدء |
| --- | --- | --- | --- | --- |
| **Windows 10/11** | x64 وARM64 | `hycli-0.1.0-win32-*-setup.exe` | `hycli-0.1.0-win32-*.tar.gz` | شغّل `*-setup.exe` (يضيف `hycli` إلى مسار `PATH` للمستخدم وقائمة ابدأ)، ثم افتح **Hycli** أو نفّذ `hycli open` في PowerShell. |
| **macOS 13+** | Apple Silicon (`arm64`) وIntel (`x64`) | `hycli-0.1.0-darwin-*.dmg` | `hycli-0.1.0-darwin-*.tar.gz` | افتح ملف `.dmg`، واسحب **Hycli.app** إلى **Applications** ثم افتحه (يربط `hycli` تلقائيًا في `~/.local/bin`). |
| **Linux** | x64 وARM64 | `hycli-0.1.0-linux-*.deb` | `hycli-0.1.0-linux-*.tar.gz` | ثبّت حزمة `.deb` (`sudo apt install ./hycli-*.deb`) أو فك ضغط `.tar.gz` وشغّل `./install.sh`، ثم نفّذ `hycli open`. |

<details>
<summary><strong>التثبيت السريع من الطرفية والبناء من المصدر</strong></summary>

**الأرشيف المحمول لنظامي Linux / macOS:**

```sh
sha256sum -c hycli-0.1.0-*.tar.gz.sha256   # أو على macOS: shasum -a 256 -c
tar -xzf hycli-0.1.0-*.tar.gz
cd hycli-0.1.0-*
./install.sh
hycli open
```

**الأرشيف المحمول لنظام Windows PowerShell:**

```powershell
tar -xzf hycli-0.1.0-win32-x64.tar.gz
cd hycli-0.1.0-win32-x64
powershell -ExecutionPolicy Bypass -File .\install.ps1
hycli open
```

**البناء من المصدر (Linux وmacOS وWindows):**

ثبّت [متطلبات البناء](../BUILD.md) ثم نفّذ:

```sh
node scripts/build.mjs
./dist/hycli open
```

نفّذ `node scripts/package-desktop.mjs` لإنتاج مثبّت `.exe` أو `.dmg` أو `.deb` الأصلي لنظام تشغيلك. راجع [دليل البناء والتغليف](../BUILD.md) و[التحقق من الإصدارات](../RELEASING.md).

</details>

### ربط موقعك الأول

1. نفّذ `hycli open` (أو انقر على أيقونة تطبيق **Hycli**) لفتح لوحة التحكم المحلية على `http://127.0.0.1:4318`.
2. افتح **اتصالات الذكاء الاصطناعي** واربط OpenAI أو Anthropic Claude أو xAI أو Z.ai أو تسجيل دخول ChatGPT (Codex) المحلي.
3. للمواقع التي تتطلب تسجيل الدخول، انقر على **الحسابات ← اعثر على تسجيل دخولي** لاستيراد جلسة المتصفح إلى الخزنة المشفرة المحلية.
4. أدخل عنوان الموقع وهدفًا اختياريًا، أو انسخ إعدادات MCP من **وكلاء البرمجة** والصقها في مساعدك الذكي.

<a id="what-your-ai-can-do"></a>
## ما يمكن لذكائك الاصطناعي فعله

يحوّل Hycli واجهات المواقع الحقيقية إلى أوامر CLI فرعية وأدوات MCP موثّقة ذاتيًا:

| واجهة الموقع | الإمكانية المُجهّزة |
| --- | --- |
| REST وOpenAPI | إجراءات قراءة وكتابة مُهيكلة ومبنية على مخططات JSON/YAML ووثائق API. |
| GraphQL | مستندات استعلام (`query`) وتعديل (`mutation`) محلّلة مع متغيرات JSON مُهيكلة. |
| صفحات HTML والصفحات المعروضة بـ JS | سجلات منظمة وروابط وترقيم صفحات وعمليات قراءة GET معروضة عبر محرك Chromium محلي. |
| نماذج JSON والنماذج المرمّزة في URL | مدخلات موثّقة وقيم افتراضية وإرسال نماذج مرتبطة بالحساب (`POST` و`PUT` و`PATCH` و`DELETE`). |

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="ثلاثة عمّال ذكاء اصطناعي، وطائر صغير مشغول جدًا." width="100%" />
</p>
<p align="center" dir="rtl"><sub>ثلاثة عمّال ذكاء اصطناعي، وطائر صغير مشغول جدًا.</sub></p>

| نوع الإجراء | سلوك التنفيذ |
| --- | --- |
| **القراءة أو البحث** | يعمل فورًا عندما يستند إلى أدلة ويُصنَّف كعملية قراءة آمنة. |
| **الإنشاء أو الإرسال أو التعديل أو الحذف** | يُكتشف ويُدمج في مواصفات الـ CLI دون أن يُنفَّذ أبدًا أثناء التجهيز. يكون متوقفًا افتراضيًا (`writes: false`). يسألك وكيل الذكاء الاصطناعي **مرة واحدة لكل CLI** عمّا إذا كنت تريد تفعيل الكتابة (`hycli writes SITE allow` أو MCP `hycli_writes`). وبمجرد تفعيله، تُنفَّذ إجراءات الكتابة مباشرةً عندما تطلبها. |
| **أثر غير واضح** | يُعلَّق للمراجعة قبل الإرسال. |
| **تحدي مصادقة أو حدّ طلبات** | يبدّل استراتيجية النقل فورًا (HTTP ← `window.fetch` داخل الصفحة ← Chromium المتخفي) أو يتوقف مؤقتًا بأمان. |

راجع [مرجع العمليات](../SITESPEC.md) للاطلاع على أشكال الطلبات المدعومة وقواعد الاستخراج.

<a id="common-concerns"></a>
## مخاوف شائعة وكيف يحلّها Hycli

يثير ربط وكيل ذكاء اصطناعي بمواقع حقيقية وحسابات مسجّلة الدخول تساؤلات جوهرية. إليك كيف يتعامل Hycli مع كل منها بالتصميم:

### 1. "هل سيقوم الذكاء الاصطناعي بنشر أو تعديل أو حذف بيانات في حسابي عن طريق الخطأ؟"
- **صفر تعديلات فعلية أثناء إنشاء الـ CLI:** أثناء تجهيز الموقع أو فحصه (`hycli prepare` و`hycli request` و`hycli recon` و`hycli check`)، يكتشف Hycli عمليات القراءة والكتابة معًا ويجمعها في مواصفات الـ CLI، لكنه **لا ينفّذ طلبات الكتابة أبدًا**. حتى التعديلات المتخفية (مسارات `GET` ذات التأثير التعديلي مثل `/delete`، أو أجسام GraphQL `mutation`، أو نماذج HTML `<form method="post">`) تُمنع من التشغيل أثناء التحقق.
- **تأكيد كتابة لمرة واحدة لكل CLI:** يبدأ كل CLI موقع جديد بوضع `writes: false`. عندما يحتاج وكيل الذكاء الاصطناعي إلى تشغيل إجراء كتابة على CLI لأول مرة، يسألك **مرة واحدة لذلك الـ CLI**: *"هل تسمح بإجراءات الكتابة لموقع `<site>`؟"* ولا يشغّل `hycli writes <site> allow` (أو MCP `hycli_writes`) إلا بعد موافقتك، مما يتيح تنفيذ الكتابة مباشرةً لذلك الـ CLI. يمكنك إلغاء إذن الكتابة في أي وقت عبر `hycli writes <site> deny` أو مفتاح **السماح بالتغييرات** في لوحة التحكم.
- **عدم إعادة محاولة الكتابة تلقائيًا:** لا تُعاد محاولة طلبات الكتابة الفاشلة أو المنقطعة تلقائيًا أبدًا. إذا انقطع الاتصال بعد إرسال تعديل، يُبلّغ Hycli عن `action_outcome_unknown` لتتحقق من الموقع بدلًا من تكرار طلب شراء أو منشور.

### 2. "هل يرسل Hycli ملفات تعريف الارتباط أو رموز الدخول أو مفاتيح API إلى نموذج الذكاء الاصطناعي؟"
- **الأسرار لا تدخل أبدًا في مطالبات LLM:** تُحفظ ملفات تعريف ارتباط المتصفح ورموز `localStorage` / `sessionStorage` ومفاتيح API حصريًا في خزنتك المحلية (مشفرة عبر حلقة مفاتيح النظام في مدير بيانات اعتماد Windows، وسلسلة مفاتيح macOS، وLinux Secret Service، أو مقيدة بصلاحيات `0600` للمستخدم الحالي فقط).
- **وصفات اتصال رمزية (`ConnectionRecipe`):** لا ترى نماذج الذكاء الاصطناعي سوى تسميات الحسابات وأسماء مفاتيح التخزين ومخططات الاستجابة المنقّحة. يبني النموذج `ConnectionRecipe` رمزيًا (يشير إلى اسم ملف تعريف الارتباط أو الرمز فقط)، ويحقن Hycli السر الحقيقي محليًا وقت التنفيذ مع إزالة الأسرار من جميع المخرجات والسجلات.
- **استيراد للقراءة فقط مقيد بالموقع:** يقرأ استيراد جلسة المتصفح ملفات تعريف الارتباط وبيانات التخزين الخاصة بالنطاق المستهدف فقط من لقطة مؤقتة للقراءة فقط. ولا يضعف أمان المتصفح أو يصدّر بيانات مواقع أخرى أبدًا.

### 3. "هل سيؤدي استخراج البيانات إلى ظهور CAPTCHA أو حظر حسابي؟"
- **لا فحص عشوائي (Fuzzing) أو تخمين بالقوة الغاشمة:** لا يخمّن Hycli قوائم عناوين نقاط النهاية ولا يفحص الخوادم الحية عشوائيًا. بل يتعلم من الوثائق المرتبطة ومخططات OpenAPI/GraphQL وحزم JS الثابتة وأشكال حركة شبكة المتصفح المرصودة.
- **حركة مرور متسلسلة ومتحكَّم في سرعتها:** تُنفَّذ الطلبات بالتسلسل وتخضع لحدود سرعة لكل موقع ولكل حساب، مع احترام ترويسة `Retry-After` وتقييد محاولات إعادة القراءة.
- **قاطع دائرة الاستراتيجية والتنفيذ داخل المتصفح الأصلي:** عندما يصطدم طلب HTTP عادي بخطأ 403/429 أو تحدي بوتات (مثل Cloudflare أو Akamai Bot Manager)، لا يكرر Hycli الطلب المحظور في حلقة مفرغة. بل يبدّل قاطع الدائرة وسيلة النقل فورًا — فيُنفّذ `window.fetch(..., { credentials: 'include' })` داخل سياق متصفحك الموثّق أو يستخدم محرك Chromium المتخفي المحلي — أو يتوقف مؤقتًا بأمان لحماية حسابك.

### 4. "هل يمكن لإعادة التوجيه تسريب جلستي، أو هل يمكن لمواصفات مشتركة كشف مسارات حاسوبي؟"
- **ربط صارم بالمصدر (Origin):** تُقفل ترويسات المصادقة وملفات تعريف الارتباط على المصدر المستهدف وتُحذف فورًا عند إعادة التوجيه عبر مصادر مختلفة (Cross-origin). كما تُحظر نطاقات عناوين IP الخاصة والمحلية افتراضيًا ما لم يُمرَّر الخيار `--allow-local` صراحةً.
- **مواصفات قابلة للنقل:** تبقى مسارات الملفات التنفيذية للمتصفح والملفات الشخصية المحلية ضمن حالة الجهاز المحلي فقط ولا تُكتب أبدًا في ملفات `SiteSpec` القابلة للنقل.

### 5. "هل سيهدر الذكاء الاصطناعي الرموز (Tokens) في إعادة المحاولة عند انتهاء صلاحية تسجيل دخولي؟"
- **فحص جاهزية حتمي بدون ذكاء اصطناعي (`hycli check`):** يعيد `hycli check` (وMCP `hycli_check`) تشغيل سلاسل القراءة الموثّقة دون استدعاء نموذج LLM، ويفصل بدقة بين `signed_out` (أعد ربط حسابك) و`site_changed` (أصلح المواصفات) و`blocked` و`unreachable`.

<a id="ai-connections"></a>
## اتصالات الذكاء الاصطناعي

| المزوّد | المصادقة |
| --- | --- |
| ChatGPT · مفتاح API | مفتاح OpenAI API |
| Claude | مفتاح Anthropic API |
| xAI | مفتاح xAI API |
| Z.ai / Z.ai Coding Plan | مفتاح Z.ai API |
| ChatGPT · تسجيل الدخول | تسجيل دخول ChatGPT محلي عبر بروتوكول Codex app-server الرسمي |

يعمل اتصال تسجيل دخول Codex في خيط مؤقت مع تعطيل تنفيذ الملفات والطرفية والمتصفح وخوادم MCP الخارجية؛ ولا يقرأ Hycli رموز OAuth الخاصة بـ Codex أو ينسخها أبدًا.

<a id="use-hycli-with-your-ai"></a>
## استخدم Hycli مع مساعدك الذكي

أضف Hycli إلى عميل MCP لديك (**وكلاء البرمجة ← عرض إعدادات الاتصال** يعرض المسار الدقيق لنظام تشغيلك):

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

استخدم `hycli mcp --sites-only` لإتاحة أدوات المواقع المثبّتة فقط، أو أضف `--only-site SITE` لقصره على موقع واحد. يوفّر خادم MCP الأدوات `hycli_prepare` و`hycli_writes` (يفعّل الكتابة مرة واحدة لكل CLI بعد سؤالك) و`hycli_run` و`hycli_check` و`hycli_result` و`hycli_activity` — ولا يوفّر أبدًا أداة لقراءة بيانات الاعتماد.

ومن الطرفية، يعمل كل مسار عمل بالطريقة نفسها:

```sh
hycli prepare https://your-website.example --intent "العثور على المراجع المحفوظة"
hycli request SITE "أضف تعديل المسودات والنشر مع محتوى المنشور الكامل"
hycli describe SITE
hycli SITE ACTION --help
hycli writes SITE allow
hycli SITE ACTION --query "design systems"
hycli check SITE
```

[دليل الوكلاء](../../agent/AGENTS.md) · [مهارة Hycli](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## تسجيل الدخول عبر المتصفح

يكتشف Hycli الملفات الشخصية المحلية لمتصفحات Chrome وChromium وEdge وBrave وVivaldi وArc وWhale وFirefox (بالإضافة إلى محركات Chromium المتخفية المثبّتة محليًا) على أنظمة Windows وmacOS وLinux، ويستورد جلسة الموقع المطلوب فقط إلى الخزنة المحلية. وتُدمج الملفات الشخصية التي تشترك في حساب الموقع الموثّق نفسه تلقائيًا لمنع التكرار.

إذا كانت الجلسة مقفلة بتشفير Windows App-Bound أو حاويات خاصة أو متصفح غير مستورد، فاستخدم **الحسابات ← ربط حساب** لتسجيل الدخول عبر متصفحك، أو اربط [إضافة المتصفح المرافقة](../../browser-companion/guide.html)، أو استورد ملف تعريف ارتباط بتنسيق JSON/Netscape في لوحة التحكم (لا تلصق ملفات تعريف الارتباط في محادثة الذكاء الاصطناعي أبدًا).

<a id="languages"></a>
## اللغات

تدعم لوحة التحكم وأوصاف الإجراءات التي ينشئها الذكاء الاصطناعي اللغات العشرين المرتبطة في أعلى ملف README هذا، بما في ذلك اللغة العربية من اليمين إلى اليسار (RTL).

<a id="local-data"></a>
## البيانات المحلية وحدود الطلبات

تُحفظ تعريفات المواقع وبيانات الاعتماد المشفرة وسجلات النشاط في مجلد بيانات نظام التشغيل لديك (`%LOCALAPPDATA%\hycli` على Windows، و`~/Library/Application Support/hycli` على macOS، و`~/.local/share/hycli` على Linux، أو `HYCLI_DATA_DIR`). يمكنك فحص حالة حماية الخزنة النشطة في أي وقت ضمن **الإعدادات**.

<a id="contributing"></a>
## التطوير والتحقق

```sh
npm --prefix dashboard ci
npm --prefix dashboard run check
npm --prefix dashboard run build
cargo test --all-targets --locked
cargo build --locked --bin hycli --example dashboard_fixture
node scripts/test-e2e.mjs --full
```

تعمل جميع الاختبارات على مواقع محلية وبيانات اختبار اصطناعية — لا تختبر عمليات الكتابة على حساب خارجي حقيقي أبدًا. راجع [مصادر المواد المرئية](../images/README.md).

## الترخيص والاستخدام المقصود

يُرخَّص Hycli بموجب رخصة [Apache-2.0](../../LICENSE). استخدمه فقط مع المواقع والحسابات المصرّح لك بالوصول إليها. يُقدَّم البرنامج "كما هو" دون أي ضمان؛ ولا يتحمل المؤلفون والمساهمون أي مسؤولية عن أي خسائر أو قيود أو حظر للحسابات ينشأ عن استخدامه.

## Star History

<a href="https://www.star-history.com/#Hybirdss/Hycli&Date">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date&theme=dark" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
 </picture>
</a>
