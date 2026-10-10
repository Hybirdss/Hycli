<p align="center">
<img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="Hycli — Web siteleri, AI için hazır." width="100%" />
</p>

<p align="center"><strong>Web sitelerini AI'ınızın kullanabileceği araçlara dönüştürün.</strong></p>

<p align="center">
<a href="#get-started">Başlangıç</a> · <a href="#what-your-ai-can-do">AI'ınız neler yapabilir?</a> · <a href="#ai-connections">AI bağlantıları</a> · <a href="#use-hycli-with-your-ai">AI ile kullanın</a>
</p>

<details>
<summary>Kendi dilinizde okuyun · 20 dil</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

Bir web sitesi ekleyin. Hycli, AI'ınızın kullanabileceği işlemleri hazırlar ve her birinin ne yaptığını açıklar. Web siteleriniz, giriş bilgileriniz ve sonuçlarınız yerel bir panelde bir arada durur.

| Bağlayın | Hazırlayın | Kullanın |
| --- | --- | --- |
| Web sitesi ekleyip AI'ınızı seçin. | İlerlemeyi takip edin ve kullanılabilir işlemleri görün. | İşlem çalıştırın veya AI asistanınıza verin. |

<p align="center"><img src="../../docs/images/dashboard.png" alt="Web sitesi kartlarını, kullanılabilir işlemleri ve görevlerin ilerlemesini gösteren Hycli paneli." width="100%" /></p>
<p align="center"><sub>Örnek araçlar ve hesaplar içeren örnek çalışma alanı.</sub></p>

<a id="get-started"></a>
Bir alan adı ve isterseniz yanındaki alana bir amaç girin. Yapay zekâ önce siteyi anlayıp yararlı iş akışları seçer, ardından ön koşulları, girdileri ve sonuçları kontrol eder. Eksik adımlar varsa site inceleme gerektiriyor olarak kalır.

## Başlangıç

**[İndir v0.1.0 · Linux x64](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)** · glibc ≥ 2.39 · [SHA-256](https://github.com/Hybirdss/Hycli/releases/download/v0.1.0/hycli-0.1.0-linux-x64.tar.gz.sha256)

Hycli, yerel bir web paneli olan Rust uygulamasıdır. Bu depo kopyasında paneli ve çalıştırılabilir dosyayı derleyin:

```sh
node scripts/build.mjs
./dist/hycli dashboard
```

Panel `http://127.0.0.1:4318` adresinde açılır. Tarayıcıyı açmadan adresi yazdırmak için `hycli dashboard --no-open`, başka bir port seçmek için `--port 4320` kullanın. Web sitesi motoru ve panel aynı çalıştırılabilir dosyanın içindedir.

1. **AI bağlantıları** bölümünü açıp bir sağlayıcı bağlayın.
2. Web sitesi adresini ekleyin ve hazırlığı yapacak AI'ı seçin.
3. Kullanılabilir işlemleri inceleyin. Bir işlemi panelden çalıştırın veya MCP yapılandırmasını bir kez kopyalamak için **Kodlama aracıları** bölümünü açın.

Giriş gerektiren bir site için **Hesaplar** bölümünden hesap bağlayın. Bir web sitesinde birden fazla kayıtlı hesap bulunabilir; araçların hangi hesabı kullanacağını siz seçersiniz.

<a id="what-your-ai-can-do"></a>
## AI'ınız neler yapabilir?

Hycli, desteklenen web sitesi işlemlerini adı ve veri türleri tanımlanmış araçlara dönüştürür. AI'ın yazdığı açıklamalar, her işlemin ne yaptığını, hangi bilgilere ihtiyaç duyduğunu ve ne döndürdüğünü anlatır.

Üç bağımsız AI aracısı okuma ve arama, yararlı iş akışları ve hesap kanıtları üzerinde çalışır. Panel, tamamlanan aşamaları, her aracının durumunu, geçen süreyi ve anlaşılır iş notlarını gösterir. CLI ve MCP çalışmaları aynı etkinlik geçmişinde görünür.

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="Üç çalışan. Çok meşgul küçük bir kuş." width="100%" />
</p>
<p align="center"><sub>Üç çalışan. Çok meşgul küçük bir kuş.</sub></p>

Hazırlık, web sitesinde gerçekten bulunan bilgilere dayanır: bağlantı verilen belgeler, API şemaları, yayımlanmış JavaScript ve tarayıcı eklentisinin gözlemlediği istek yapıları. Bir işlemi kontrol etmek için sınırlı okuma işlemleri yapabilir. Test kayıtları oluşturmaz, içerik düzenlemez, nesne silmez, uzun uç nokta listeleri tahmin etmez veya çalışan bir sunucuya rastgele test girdileri göndermez.

| İşlem | Davranış |
| --- | --- |
| Okuma veya arama | İşlem eldeki bulgularla destekleniyor ve okuma olarak sınıflandırılmışsa kendi başına çalışır. |
| Oluşturma, gönderme, düzenleme veya silme | Kullanıcının onayı için web sitesini, hesabı, işlemi ve kesinleşmiş girdileri gösterir. |
| Etkisi belirsiz işlem | İstek gönderilmeden önce inceleme gerektirir. |
| Kimlik doğrulama, ek doğrulama veya istek sınırı | Etkilenen istekleri duraklatır; yeniden bağlanmanızı veya beklemenizi sağlar. |

Onay, tek bir belirli istek için geçerlidir; beş dakika sonra sona erer ve yalnızca bir kez kullanılabilir. Girdilerin, hesabın, kayıtlı giriş bilgilerinin veya kurulu araç tanımının değişmesi onayı geçersiz kılar. CLI, MCP ve panel aynı sınırı uygular. Değişiklik yapan istekler hiçbir zaman otomatik olarak yeniden denenmez.

Destek web sitesine bağlıdır. Kullanılabilir belgesi veya gözlemlenmiş işlemi olmayan bir sayfa, giriş yapılmış tarayıcı, sağlanan bir SiteSpec veya ek hazırlık gerektirebilir. Hycli, her web sitesinin hazır bir API'ı olduğunu varsaymak yerine destekleyebildiği işlemleri bildirir.

### Desteklenen özellikler

JSON veya YAML OpenAPI, REST, GraphQL ve JSON ya da URL kodlamalı form istekleri desteklenir. HTML kayıtları, bağlantılar ve sonraki sayfa bilgisi gözlemlenen seçicilerle çıkarılabilir; yerel Chromium ile işlenen GET sayfaları da okunabilir. Hazırlık sırasında bir okuma başarısız olursa yapay zekâ kanıtları inceler, tanımı düzeltir ve yeniden doğrular.

Rastgele tarayıcı tıklama dizileri, multipart yüklemeleri ve ikili dosya indirmeleri henüz uygulanmadı. Paketler hesapları veya önceden üretilmiş CLI tanımlarını içermez.

[İşlem biçimi](../SITESPEC.md) · [Paket doğrulaması](../RELEASING.md)

<a id="ai-connections"></a>
## AI bağlantıları

| Bağlantı | Giriş |
| --- | --- |
| ChatGPT · API key | OpenAI API anahtarınız |
| Claude | Anthropic API anahtarınız |
| xAI | xAI API anahtarınız |
| Z.ai | Z.ai API anahtarınız |
| Z.ai Coding Plan | Z.ai Coding Plan API anahtarınız |
| ChatGPT · login | Kurulu Codex app-server tarafından yönetilen ChatGPT girişi |

Panelden bir model seçin; hesabınıza özel olarak sunulan modelleri de kullanabilirsiniz. Bağlantı kontrolleri sağlayıcıyı ve seçilen modeli doğrular. API sağlayıcıları kullanımı sağlayıcı hesabınıza faturalandırır.

Codex bağlantısı resmi app-server protokolünü kullanır. Hycli, Codex OAuth belirteçlerini okumaz veya kopyalamaz. Model, dosya, kabuk, tarayıcı, uygulama ve MCP çalıştırma yetkileri kapatılmış geçici bir konuşmada çalışır. Web sitesi hazırlığı Hycli'nin denetimli okuma araçlarıyla yapılır.

<a id="use-hycli-with-your-ai"></a>
## Hycli'yi AI'ınızla kullanın

**Kodlama aracıları → Bağlantı ayarlarını görüntüle**

Genel MCP bağlantısı, siteleri hazırlamayı ve eylemleri çalıştırmayı kapsar. Ajanınızı bir kez bağlayın; eksik araçları hazırlayabilsin, ilerlemeyi izleyebilsin ve yeni eylemleri kullanabilsin.

```sh
hycli mcp
```

Yalnızca kurulu eylemleri sunmak için `--sites-only` kullanın. Aşağıdaki bağlantı tek bir siteyle sınırlıdır.

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

Ajanın PATH değişkeninde `hycli` yoksa panoda gösterilen yürütülebilir dosya yolunu kullanın.

Aynı akış CLI üzerinden de kullanılabilir. Örnek URL, `SITE` ve `ACTION` yerine sitenizi ve `describe` komutunun döndürdüğü adları yazın.

```sh
hycli prepare https://your-website.example --intent "Kaydedilmiş kaynakları bul"
hycli request SITE "Add draft editing and publishing with full post content"
hycli describe
hycli describe SITE ACTION
hycli SITE ACTION --help
hycli run SITE ACTION --arg query="design systems"
hycli jobs show JOB_ID --watch
```

MCP üzerinden URL ve `intent` değerini `hycli_prepare` aracına verin, ardından `hycli_job` veya `hycli_result` ile işi izleyin. `hycli_run`, istemci araç listesini yenilemeden önce bile yeni eylemleri çalıştırabilir. Dahili kimlikleri önce ilgili listeleme veya arama eylemleriyle bulun.

Değişiklikler, panoda incelenecek bir onay kaydı döndürür. İsteği tekrarlamadan bu kaydın sonucunu izleyin. Başarısız okumalar veya beklenmeyen yanıtlar CLI üzerinde de hata durumuyla sonuçlanır.

[Ajan kılavuzu](../../agent/AGENTS.md) · [Hycli becerisi](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## Tarayıcıdan giriş

Web sitesi hazırlığı, mevcut işletim sistemini, çalışan ve kayıtlı tarayıcıları ve kullanılabilir profilleri inceleyerek başlar. Yapay zekâ, belgeleri okumak, sayfaları görüntülemek, bir site oturumunu içe aktarmak ve bağlantıyı doğrulamak için bu imkânlar arasından seçim yapar. Tarayıcı ve profil yolları yerel makinede kalır; oluşturulan web sitesi tanımı geliştiricinin kurulum yollarına bağlı değildir.

Chrome, Chromium, Edge, Brave ve Firefox çerezleri ile kaynak depolama verileri erişilebildiğinde içe aktarılabilir. Yalnızca istenen web sitesinin oturumu Hycli’nin yerel kasasına girer. Hycli doğrulanmış tek bir kimliği otomatik olarak seçer; aynı kimliğe ait profiller tek bir hesap seçeneği sayılır. Mevcut hesap seçimi korunur; farklı doğrulanmış kimlikler arasında seçim yapılması gerekir.

**Hesaplar → Hesap bağla** bölümündeki **Oturum açma bilgilerimi bul** bu aramayı tekrarlar. Kullanılabilir oturum bulunmazsa **Web sitesinin giriş sayfasını aç** siteyi varsayılan tarayıcınızda açar. İletişim kutusu açıkken Hycli yeniden kontrol eder ve yalnızca seçilen hesap doğrulandıktan sonra hazırlığa devam eder. Yalnızca sekme açılması başarılı giriş anlamına gelmez.

İşletim sistemi tarafından korunan, tarayıcıya bağlı, özel veya kapsayıcı oturumlar tarayıcı yardımcısını gerektirebilir. Yardımcı sekmesini seçip bağlantı kodu oluşturun; sonra yardımcıyı giriş yapılmış profilde açın, kodu girin ve web sitesine erişim izni verin. Doğrudan içe aktarma Windows App-Bound korumasını aşmaz veya tarayıcı profilini zayıflatmaz.

- **Chrome ve Edge:** paketi açın ve tarayıcının uzantılar sayfasında **Paketlenmemiş öğe yükle** seçeneğini kullanın.
- **Firefox:** Firefox paketini ve `about:debugging` sayfasındaki **Geçici eklenti yükle** seçeneğini kullanın. Geçici eklentiler Firefox yeniden başladığında kaldırılır. Bu sürümde mağaza üzerinden imzalı dağıtım sunulmaz.
- **Çerez dosyası:** alternatif olarak JSON ve Netscape çerez dosyaları içe aktarılabilir. Dosyayı doğrudan panelden içe aktarın; içeriğini bir AI konuşmasına yapıştırmayın.

Tarayıcı, oturum verilerini doğrudan yerel kimlik bilgisi kasasına aktarır. Modeller hesap etiketlerini, yerel anahtar adlarını ve yapısını, ayrıca kimlik bilgisi değerleri çıkarılmış araç sonuçlarını alır. Bu değerleri almadan yerel değerlere başvuran bir bağlantı tarifi oluşturabilirler. Çerez kapsamı, yolları, son kullanma tarihi ve desteklenen bölümleme bilgileri dikkate alınır; kimlik doğrulama başlıkları özgün kaynaklarıyla sınırlı kalır.

Hesap kimliği, web sitesinin mevcut hesaba ilişkin gerçek yanıtından alınır. Tarayıcı profili adı, doğrulanmış web sitesi kimliği olarak kabul edilmez. Hesap henüz tanınmamışsa Hycli bunu belirtir. Bağlandıktan sonra normal gezinme, hesap yanıtını ve kullanılabilir isteklerin yapılarını sağlayabilir; istek değerleri, başlıklar veya yanıt gövdeleri hazırlığı yapan modele gösterilmez.

Bir site, ayrı ve belgelenmiş API kimlik bilgileri ya da yerel olarak bulunmayan bir tarayıcı özelliği gerektirebilir. Hazırlık gerçek bağlantı ve okuma sonuçlarını kaydeder. Ana sayfanın alınması veya API’den bir giriş sayfası dönmesi entegrasyonun hazır olduğu anlamına gelmez. [Tarayıcı kurulum kılavuzu](../../browser-companion/guide.html) yardımcı uzantı akışını ve sınırlarını açıklar.

<a id="languages"></a>
## Diller

Panel, sağdan sola yazılan Arapça dahil yukarıda bağlantısı verilen 20 dili destekler. Başlangıç dili İngilizcedir. Seçiminiz bu cihazda kaydedilir ve AI açıklamaları seçilen dilde hazırlanabilir.

<a id="local-data"></a>
## Yerel veriler ve istek sınırları

Web sitesi tanımları, hesap üst verileri ve etkinlik yerel veri dizininde kalır. Farklı bir konum seçmek için `HYCLI_DATA_DIR` ayarlayın. Etkin konumu ve kimlik bilgilerinin nasıl korunduğunu **Ayarlar** bölümünden görebilirsiniz.

Kimlik bilgisi kasası, kullanılabildiğinde işletim sisteminin anahtar deposuyla korunan bir şifreleme anahtarı kullanır. Anahtar deposu yoksa yalnızca dosya izinleriyle korunan bir depolama kullandığını açıkça bildirir; özel dizinler ve kimlik bilgisi dosyaları yalnızca mevcut işletim sistemi kullanıcısına açıktır. Kilidi açılamayan mevcut şifreli kasa sessizce değiştirilmez.

İstekler her web sitesi ve hesap için sırayla, belirli aralıklarla gönderilir. Ayrıca web sitesinin tamamı için ek bir sınır uygulanır. Hycli, `Retry-After` değerini dikkate alır, yanıt boyutlarını ve okuma tekrarlarını sınırlar, kimlik doğrulama hatalarında veya sitenin ek doğrulama istemesi durumunda duraklar. Web sitesi hazırlığı sırasında Hycli hesap değiştirmez, cihaz parmak izini taklit etmez veya doğrulama adımlarını atlatmaz; motorun tanılama davranışı farklı olabilir. Bu önlemler gereksiz yükü azaltır; bir web sitesinin hesabı hiçbir zaman kısıtlamayacağını garanti etmez.

Özel ve yerel web sitesi adresleri varsayılan olarak devre dışıdır. Güvendiğiniz, kendi sunucunuzda barındırılan bir site için paneli veya MCP sunucusunu açıkça `--allow-local` seçeneğiyle başlatın.

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

Testler, yapay yerel web siteleri ve sağlayıcı yanıtları kullanır. Onayın isteğe bağlanmasını ve tekrar kullanımın engellenmesini, gizli bilgilerin çıkarılmasını, yönlendirmeleri, istek sınırlarını, değişikliklerin yeniden denenmemesini, tarayıcı oturumunun korunmasını ve sağlayıcı yanıtlarının beklenen yapıya uygunluğunu kapsar. Yalnızca Hycli'yi test etmek için gerçek bir hesapta içerik oluşturmayın, değiştirmeyin veya silmeyin.

Ekran görüntüleri ve GIF, yalıtılmış örnek veriler kullanır. [Görsel kaynaklarına](../images/README.md) bakın.

## Lisans ve kullanım amacı

Hycli, [Apache-2.0](../../LICENSE) lisansı altında sunulur.

Hycli, web sitelerinin kullanımını kolaylaştıran komut satırı araçları oluşturmak ve kullanmak için tasarlanmıştır. Erişim yetkiniz olan web siteleri ve hesaplarla kullanın.

Yazılım olduğu gibi, garanti verilmeksizin sunulur. Yasaların izin verdiği ölçüde yazarları ve katkıda bulunanlar, kullanımından doğan kayıplardan veya diğer sorunlardan sorumlu değildir. Yazılımı nasıl kullandığınız sizin sorumluluğunuzdadır.

Yasaların izin verdiği ölçüde, yazarlar ve katkıda bulunanlar Hycli kullanımından kaynaklanan hesap yasaklamaları, askıya almaları veya kısıtlamalarından sorumlu değildir. Hycli'yi bilgisayar korsanlığı, yetkisiz erişim veya saldırılar için kullanmayın.
