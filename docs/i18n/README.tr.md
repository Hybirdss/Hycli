<p align="center">
  <img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="Hycli — Yapay zekâ için hazır web siteleri." width="100%" />
</p>

<p align="center">
  <strong>Herhangi bir web sitesini yapay zekâ ajanınız için tipli bir CLI ve MCP sunucusuna dönüştürün.</strong><br/>
  <sub>Gerçek tarayıcı oturumunuzu kullanarak, LLM'lere sıfır çerez sızıntısı ve yerleşik bot koruması dayanıklılığıyla okuyun, arayın, oluşturun, düzenleyin ve otomatikleştirin.</sub>
</p>

<p align="center">
  <a href="https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0">İndir (Windows · macOS · Linux)</a> ·
  <a href="https://github.com/Hybirdss/Hycli/issues">GitHub üzerinden yardım alın</a> ·
  <a href="#try-it-in-30-seconds">30 saniyede deneyin</a> ·
  <a href="#get-started">Başlangıç</a> ·
  <a href="#what-your-ai-can-do">Yapay zekânız neler yapabilir</a> ·
  <a href="#common-concerns">Sıkça duyulan endişeler</a> ·
  <a href="#use-hycli-with-your-ai">Yapay zekânızla kullanın</a>
</p>

<details>
<summary>Kendi dilinizde okuyun · 20 dil</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

Hycli'ye herhangi bir web sitesi URL'si ve ne yapmak istediğinizi verin. Üç yapay zekâ çalışanı; dokümantasyonu, önyüz JS paketlerini ve gerçek tarayıcı ağ trafiğini inceleyerek hem okuma hem de yazma işlemlerini kapsayan tipli bir CLI ve MCP araç seti oluşturur — üstelik hazırlık sırasında hesabınızda tek bir canlı değişiklik bile yapmaz.

<p align="center">
  <img src="../../docs/images/dashboard.png" alt="Web sitesi kartlarını, kullanılabilir eylemleri ve görev ilerlemesini gösteren Hycli kontrol paneli." width="100%" />
</p>
<p align="center"><sub>Web siteleriniz, hesaplarınız, yazma izinleriniz ve yapay zekâ ajanlarınız için tek bir yerel kontrol paneli ve CLI.</sub></p>

<a id="try-it-in-30-seconds"></a>
## 30 saniyede deneyin

```sh
# 1. Hycli'ye bir web sitesi gösterin ve amacınızı açıklayın (okuma ve yazmaları hazırlar; sıfır canlı değişiklik)
hycli prepare https://your-website.example --intent "Gönderileri ara, taslakları kaydet ve makaleleri yayınla"

# 2. Hazırlanan tüm okuma, yazma ve çok adımlı iş akışlarını görün
hycli describe your-website

# 3. Okuma işlemlerini hemen çalıştırın
hycli your-website search-posts --query "sürüm notları"

# 4. Oluşturma, düzenleme veya silme yapmaya hazır olduğunuzda bu CLI için yazma iznini bir kez etkinleştirin
hycli writes your-website allow
hycli your-website create-draft --title "Hycli'den Merhaba" --body "Terminalimden otomatik olarak oluşturuldu."
```

Veya Hycli'yi **Claude Code, Cursor, Codex ya da Windsurf**'e `"command": "hycli", "args": ["mcp"]` ile bir kez bağlayın ve ajanınıza doğal dilde söyleyin:
> *"Hycli ile `https://...` sitesini hazırla, hangi işlemlerin kullanılabilir olduğunu göster ve yeni bir gönderi taslağı oluştur."*

<a id="get-started"></a>
## Başlangıç

Hycli, hem web sitesi motorunu hem de yerel kontrol panelini (`http://127.0.0.1:4318`) tek bir yerel çalıştırılabilir dosyada barındırır. Dağıtım paketini çalıştırmak için Rust veya Node.js gerekmez.

### İndirme ve çalıştırma (Windows, macOS, Linux)

Platformunuza uygun yerel yükleyiciyi veya taşınabilir arşivi **[GitHub Releases (v0.1.0)](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)** sayfasından indirin:

| Platform | Mimari | Yerel Yükleyici | Taşınabilir Arşiv | Nasıl başlatılır |
| --- | --- | --- | --- | --- |
| **Windows 10/11** | x64 ve ARM64 | `hycli-0.1.0-win32-*-setup.exe` | `hycli-0.1.0-win32-*.tar.gz` | `*-setup.exe` dosyasını çalıştırın (kullanıcı `PATH` değişkenine ve Başlat Menüsü'ne `hycli` ekler), ardından **Hycli**'yi başlatın veya PowerShell'de `hycli open` komutunu çalıştırın. |
| **macOS 13+** | Apple Silicon (`arm64`) ve Intel (`x64`) | `hycli-0.1.0-darwin-*.dmg` | `hycli-0.1.0-darwin-*.tar.gz` | `.dmg` dosyasını açın, **Hycli.app** uygulamasını **Applications** klasörüne sürükleyin ve açın (`hycli` komutunu otomatik olarak `~/.local/bin` içine bağlar). |
| **Linux** | x64 ve ARM64 | `hycli-0.1.0-linux-*.deb` | `hycli-0.1.0-linux-*.tar.gz` | `.deb` paketini kurun (`sudo apt install ./hycli-*.deb`) veya `.tar.gz` arşivini çıkarıp `./install.sh` dosyasını çalıştırın, ardından `hycli open` komutunu çalıştırın. |

<details>
<summary><strong>Terminalden hızlı taşınabilir kurulum ve kaynaktan derleme</strong></summary>

**Linux / macOS taşınabilir arşiv:**

```sh
sha256sum -c hycli-0.1.0-*.tar.gz.sha256   # macOS'ta: shasum -a 256 -c
tar -xzf hycli-0.1.0-*.tar.gz
cd hycli-0.1.0-*
./install.sh
hycli open
```

**Windows PowerShell taşınabilir arşiv:**

```powershell
tar -xzf hycli-0.1.0-win32-x64.tar.gz
cd hycli-0.1.0-win32-x64
powershell -ExecutionPolicy Bypass -File .\install.ps1
hycli open
```

**Kaynaktan derleme (Linux, macOS, Windows):**

[Derleme önkoşullarını](../BUILD.md) kurun ve şu komutları çalıştırın:

```sh
node scripts/build.mjs
./dist/hycli open
```

İşletim sisteminiz için yerel `.exe`, `.dmg` veya `.deb` yükleyicisini üretmek üzere `node scripts/package-desktop.mjs` komutunu çalıştırın. Ayrıntılar için [derleme ve paketleme](../BUILD.md) ile [sürüm doğrulama](../RELEASING.md) belgelerine bakın.

</details>

### İlk web sitenizi bağlayın

1. `hycli open` komutunu çalıştırarak (veya **Hycli** uygulama simgesine tıklayarak) `http://127.0.0.1:4318` adresindeki yerel kontrol panelini açın.
2. **Yapay zekâ bağlantıları** bölümünü açın ve OpenAI, Anthropic Claude, xAI, Z.ai veya yerel ChatGPT (Codex) oturumunuzu bağlayın.
3. Oturum açılması gereken siteler için **Hesaplar → Oturumumu bul** seçeneğine tıklayarak tarayıcı oturumunuzu yerel şifreli kasaya aktarın.
4. Bir web sitesi URL'si ve isteğe bağlı amacınızı girin ya da **Kodlama ajanları** bölümündeki MCP yapılandırmasını kopyalayıp yapay zekâ asistanınıza yapıştırın.

<a id="what-your-ai-can-do"></a>
## Yapay zekânız neler yapabilir

Hycli, gerçek web sitesi arayüzlerini kendi kendini belgeleyen CLI alt komutlarına ve MCP araçlarına dönüştürür:

| Web sitesi arayüzü | Hazırlanan yetenek |
| --- | --- |
| REST ve OpenAPI | JSON/YAML şemalarına ve API belgelerine dayalı tipli okuma ve yazma işlemleri. |
| GraphQL | Tipli JSON değişkenlerine sahip ayrıştırılmış sorgu (`query`) ve mutasyon (`mutation`) belgeleri. |
| HTML ve JS ile işlenen sayfalar | Yerel bir Chromium motoru aracılığıyla yapılandırılmış kayıtlar, bağlantılar, sayfalama ve işlenmiş GET okumaları. |
| JSON ve URL kodlu formlar | Belgelenmiş girdiler, varsayılan değerler ve hesap kapsamlı form gönderimleri (`POST`, `PUT`, `PATCH`, `DELETE`). |

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="Üç yapay zekâ çalışanı ve çok meşgul küçük bir kuş." width="100%" />
</p>
<p align="center"><sub>Üç yapay zekâ çalışanı ve çok meşgul küçük bir kuş.</sub></p>

| İşlem türü | Yürütme davranışı |
| --- | --- |
| **Okuma veya arama** | Kanıtlara dayandırıldığında ve güvenli okuma olarak sınıflandırıldığında anında çalışır. |
| **Oluşturma, gönderme, düzenleme veya silme** | Hazırlık sırasında asla çalıştırılmadan keşfedilir ve CLI tanımına eklenir. Varsayılan olarak kapalıdır (`writes: false`). Yapay zekâ ajanınız yazma işlemlerini etkinleştirmek için **CLI başına bir kez** onayınızı ister (`hycli writes SITE allow` veya MCP `hycli_writes`). Etkinleştirildikten sonra yazma işlemleri siz istediğinizde doğrudan çalışır. |
| **Belirsiz etki** | Gönderilmeden önce inceleme için bekletilir. |
| **Kimlik doğrulama engeli veya hız sınırı** | Aktarım stratejisini anında değiştirir (HTTP → sayfa içi `window.fetch` → gizli Chromium) veya güvenli bir şekilde duraklatır. |

Desteklenen istek biçimleri ve veri çıkarma kuralları için [işlem referansına](../SITESPEC.md) bakın.

<a id="common-concerns"></a>
## Sıkça duyulan endişeler ve Hycli'nin çözümleri

Bir yapay zekâ ajanını gerçek web sitelerine ve oturum açılmış hesaplara bağlamak önemli soruları beraberinde getirir. Hycli'nin her birini tasarım düzeyinde nasıl çözdüğü aşağıda açıklanmıştır:

### 1. "Yapay zekâ hesabımda yanlışlıkla paylaşım yapar, veri düzenler veya silerse ne olur?"
- **CLI oluşturma sırasında sıfır canlı değişiklik:** Bir web sitesini hazırlarken veya kontrol ederken (`hycli prepare`, `hycli request`, `hycli recon`, `hycli check`), Hycli hem okuma hem de yazma işlemlerini keşfedip CLI tanımına derler ancak **asla canlı sunucuya yazma isteği göndermez**. Gizlenmiş değişiklikler bile (`/delete` gibi değiştirici yollara sahip `GET` rotaları, GraphQL `mutation` gövdeleri veya HTML `<form method="post">` eylemleri) doğrulama sırasında engellenir.
- **CLI başına tek seferlik yazma onayı:** Yeni oluşturulan her web sitesi CLI'ı `writes: false` durumuyla başlar. Bir yapay zekâ ajanının bir CLI üzerinde ilk kez yazma işlemi çalıştırması gerektiğinde, **o CLI için bir kez** size sorar: *"`<site>` için yazma işlemlerine izin verilsin mi?"* Yalnızca siz onayladıktan sonra `hycli writes <site> allow` (veya MCP `hycli_writes`) komutunu çalıştırarak o CLI için doğrudan yazma yürütmesini etkinleştirir. Yazma iznini istediğiniz zaman `hycli writes <site> deny` komutuyla veya kontrol panelindeki **Değişikliklere izin ver** anahtarıyla geri alabilirsiniz.
- **Başarısız yazmalarda otomatik yeniden deneme yok:** Başarısız olan veya kesintiye uğrayan yazma istekleri asla otomatik olarak yeniden denenmez. Bir değişiklik gönderildikten sonra bağlantı koparsa, Hycli mükerrer sipariş veya gönderi oluşturmak yerine önce web sitesini kontrol edebilmeniz için `action_outcome_unknown` durumunu bildirir.

### 2. "Hycli çerezlerimi, oturum belirteçlerimi veya API anahtarlarımı yapay zekâ modeline gönderiyor mu?"
- **Sırlar asla LLM istemlerine girmez:** Tarayıcı çerezleri, `localStorage` / `sessionStorage` belirteçleri ve API anahtarları yalnızca yerel kasanızda saklanır (Windows Kimlik Bilgisi Yöneticisi, macOS Anahtar Zinciri ve Linux Secret Service üzerinden işletim sistemi anahtarlığıyla şifrelenir veya yalnızca kullanıcıya özel `0600` izinleriyle korunur).
- **Sembolik bağlantı tarifleri (`ConnectionRecipe`):** Yapay zekâ modelleri yalnızca hesap etiketlerini, depolama anahtarı adlarını ve maskelenmiş yanıt şemalarını görür. Model sembolik bir `ConnectionRecipe` (bir çerez veya belirteç adına referans veren) oluşturur ve Hycli gerçek sırrı yürütme anında yerel olarak enjekte ederken tüm çıktılardan ve günlüklerden sırları temizler.
- **Site kapsamlı salt okunur içe aktarma:** Bir tarayıcı oturumunu içe aktarmak, geçici bir salt okunur anlık görüntüden yalnızca hedef alan adının çerezlerini ve depolama verilerini okur. Tarayıcı güvenliğini asla zayıflatmaz veya diğer web sitelerinin verilerini dışa aktarmaz.

### 3. "Veri çekme işlemi CAPTCHA tetikler mi veya hesabımın yasaklanmasına yol açar mı?"
- **Fuzzing veya kaba kuvvet taraması yok:** Hycli asla uç nokta kelime listelerini tahmin etmez veya canlı sunuculara fuzzing uygulamaz. Yalnızca bağlantılı dokümantasyondan, OpenAPI/GraphQL şemalarından, statik JS paketlerinden ve gözlemlenen tarayıcı ağ yapılarından öğrenir.
- **Sıralı ve hız kontrollü trafik:** İstekler web sitesi ve hesap bazında sıraya alınır ve hız sınırına tabi tutulur; `Retry-After` başlıklarına uyulur ve okuma yeniden denemeleri sınırlandırılır.
- **Strateji devre kesicisi ve gerçek tarayıcı yürütmesi:** Düz HTTP istekleri 403/429 hatasına veya bot korumasına (Cloudflare, Akamai Bot Manager) takıldığında, Hycli engellenen isteği döngüye sokmaz. Devre kesicisi aktarımı anında değiştirir — kimliği doğrulanmış tarayıcı bağlamınızın içinde `window.fetch(..., { credentials: 'include' })` çalıştırır veya yerel bir gizli Chromium motoruna geçer — ya da hesabınızı korumak için güvenli bir şekilde duraklatır.

### 4. "Bir yönlendirme oturumumu sızdırabilir mi veya paylaşılan bir tanım bilgisayarımdaki dosya yollarını ifşa edebilir mi?"
- **Sıkı kaynak (Origin) bağlama:** Kimlik doğrulama başlıkları ve çerezler hedef kaynağa kilitlenir ve kaynaklar arası (cross-origin) yönlendirmelerde anında kaldırılır. Özel ve yerel ağ IP aralıkları, `--allow-local` açıkça belirtilmediği sürece varsayılan olarak engellenir.
- **Taşınabilir tanımlar:** Yerel tarayıcı çalıştırılabilir dosya ve profil yolları yalnızca cihaz içi durumda kalır ve taşınabilir `SiteSpec` dosyalarına asla yazılmaz.

### 5. "Oturumumun süresi dolduğunda yapay zekâ yeniden deneyerek jeton (token) israf eder mi?"
- **Yapay zekâ olmadan deterministik sağlık kontrolü (`hycli check`):** `hycli check` (ve MCP `hycli_check`), bir LLM çağırmadan doğrulanmış okuma zincirlerini yeniden oynatır ve `signed_out` (hesabınızı yeniden bağlayın) durumunu `site_changed` (tanımı onarın), `blocked` ve `unreachable` durumlarından net bir şekilde ayırır.

<a id="ai-connections"></a>
## Yapay zekâ bağlantıları

| Sağlayıcı | Kimlik Doğrulama |
| --- | --- |
| ChatGPT · API anahtarı | OpenAI API anahtarı |
| Claude | Anthropic API anahtarı |
| xAI | xAI API anahtarı |
| Z.ai / Z.ai Coding Plan | Z.ai API anahtarı |
| ChatGPT · oturum açma | Resmi Codex app-server protokolü üzerinden yerel ChatGPT oturumu |

Codex oturum bağlantısı; dosya, kabuk, tarayıcı ve harici MCP yürütmesi devre dışı bırakılmış geçici bir iş parçacığında çalışır; Hycli, Codex OAuth belirteçlerini asla okumaz veya kopyalamaz.

<a id="use-hycli-with-your-ai"></a>
## Hycli'yi yapay zekânızla kullanın

Hycli'yi MCP istemcinize ekleyin (**Kodlama ajanları → Bağlantı ayarlarını görüntüle** işletim sisteminiz için tam yolu gösterir):

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

Yalnızca kurulu web sitesi araçlarını açığa çıkarmak için `hycli mcp --sites-only` kullanın veya tek bir siteyle sınırlandırmak için `--only-site SITE` ekleyin. MCP sunucusu `hycli_prepare`, `hycli_writes` (size sorduktan sonra CLI başına bir kez yazmayı etkinleştirir), `hycli_run`, `hycli_check`, `hycli_result` ve `hycli_activity` araçlarını sağlar — ve kimlik bilgilerini okumak için asla bir araç sunmaz.

Terminalden de her iş akışı aynı şekilde çalışır:

```sh
hycli prepare https://your-website.example --intent "Kaydedilen referansları bul"
hycli request SITE "Tam gönderi içeriğiyle taslak düzenleme ve yayınlama ekle"
hycli describe SITE
hycli SITE ACTION --help
hycli writes SITE allow
hycli SITE ACTION --query "design systems"
hycli check SITE
```

[Ajan kılavuzu](../../agent/AGENTS.md) · [Hycli becerisi (skill)](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## Tarayıcı ile oturum açma

Hycli; Windows, macOS ve Linux'ta yerel Chrome, Chromium, Edge, Brave, Vivaldi, Arc, Whale ve Firefox profillerini (ve kurulu olduğunda yerel gizli Chromium ikili dosyalarını) keşfederek yalnızca istenen web sitesinin oturumunu yerel şifreli kasaya aktarır. Aynı doğrulanmış web sitesi hesabını paylaşan profiller otomatik olarak tekilleştirilir.

Bir oturum Windows App-Bound şifrelemesi, özel kapsayıcılar veya içe aktarılmamış bir tarayıcı tarafından kilitlenmişse, tarayıcınız üzerinden oturum açmak için **Hesaplar → Hesap bağla** seçeneğini kullanın, [tarayıcı eşlikçi uzantısını](../../browser-companion/guide.html) bağlayın veya kontrol panelinde bir JSON/Netscape çerez dosyası içe aktarın (çerezleri asla bir yapay zekâ sohbetine yapıştırmayın).

<a id="languages"></a>
## Diller

Kontrol paneli ve yapay zekâ tarafından oluşturulan işlem açıklamaları, sağdan sola (RTL) Arapça dahil olmak üzere bu README'nin üst kısmında bağlantısı verilen 20 dili destekler.

<a id="local-data"></a>
## Yerel veriler ve istek sınırları

Web sitesi tanımları, şifrelenmiş kimlik bilgileri ve etkinlik günlükleri işletim sisteminizin veri dizininde (Windows'ta `%LOCALAPPDATA%\hycli`, macOS'ta `~/Library/Application Support/hycli`, Linux'ta `~/.local/share/hycli` veya `HYCLI_DATA_DIR`) saklanır. Aktif kasa korumasını istediğiniz zaman **Ayarlar** altından inceleyebilirsiniz.

<a id="contributing"></a>
## Geliştirme ve doğrulama

```sh
npm --prefix dashboard ci
npm --prefix dashboard run check
npm --prefix dashboard run build
cargo test --all-targets --locked
cargo build --locked --bin hycli --example dashboard_fixture
node scripts/test-e2e.mjs --full
```

Tüm testler sentetik yerel web sitelerine ve test verilerine karşı çalışır — yazma işlemlerini asla gerçek bir üçüncü taraf hesabında test etmeyin. [Görsel materyal kaynaklarına](../images/README.md) bakın.

## Lisans ve kullanım amacı

Hycli, [Apache-2.0](../../LICENSE) lisansı altında lisanslanmıştır. Yalnızca erişim yetkiniz olan web siteleri ve hesaplarla kullanın. Garanti olmaksızın "olduğu gibi" sunulmaktadır; yazarlar ve katkıda bulunanlar, kullanımından kaynaklanan hiçbir kayıp, hesap kısıtlaması veya yasaklamadan sorumlu değildir.

## Star History

<a href="https://www.star-history.com/#Hybirdss/Hycli&Date">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date&theme=dark" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=Hybirdss/Hycli&type=Date" />
 </picture>
</a>
