<p align="center">
  <img src="../../docs/brand/concepts/hycli-shima-banner-v4.png" alt="Ubah situs web menjadi alat yang dapat digunakan AI Anda." width="100%" />
</p>

<p align="center">
  <strong>Ubah situs web menjadi alat yang dapat digunakan AI Anda.</strong>
</p>

<p align="center">
  <a href="#get-started">Mulai</a> ·
  <a href="#what-your-ai-can-do">Apa yang dapat dilakukan AI Anda</a> ·
  <a href="#ai-connections">Koneksi AI</a> ·
  <a href="#use-hycli-with-your-ai">Gunakan dengan AI</a>
</p>

<details>
<summary>Baca dalam bahasa Anda · 20 bahasa</summary>

[English](../../README.md) · [한국어](README.ko.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md) · [Русский](README.ru.md) · [Українська](README.uk.md) · [Tiếng Việt](README.vi.md) · [ไทย](README.th.md) · [العربية](README.ar.md) · [हिन्दी](README.hi.md)

</details>

Tambahkan situs web. Hycli menyiapkan tindakan yang dapat digunakan AI Anda dan menjelaskan fungsi masing-masing. Kelola situs web, informasi masuk, dan hasil dalam satu dasbor lokal.

| Hubungkan | Siapkan | Gunakan |
| --- | --- | --- |
| Tambahkan situs web dan pilih AI. | Pantau kemajuan dan lihat tindakan yang tersedia. | Jalankan tindakan atau berikan kepada asisten AI Anda. |

<p align="center">
  <img src="../../docs/images/dashboard.png" alt="Dasbor Hycli dengan kartu situs web, tindakan yang tersedia, dan kemajuan tugas." width="100%" />
</p>
<p align="center"><sub>Contoh ruang kerja dengan alat dan akun sampel.</sub></p>

<a id="get-started"></a>
Masukkan domain dan, bila perlu, tujuan di kolom sebelahnya. AI terlebih dahulu memahami situs dan memilih alur kerja yang berguna, lalu memeriksa prasyarat, masukan, dan hasil akhirnya. Jika ada langkah yang kurang, situs tetap ditandai untuk ditinjau.

## Mulai

**[Unduh v0.1.0 · Linux x64](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0)** · glibc ≥ 2.39 · [SHA-256](https://github.com/Hybirdss/Hycli/releases/download/v0.1.0/hycli-0.1.0-linux-x64.tar.gz.sha256)

Hycli adalah aplikasi Rust dengan dasbor web lokal. Dari salinan kode sumber ini, bangun dasbor dan berkas eksekusinya:

```sh
node scripts/build.mjs
./dist/hycli dashboard
```

Dasbor terbuka di `http://127.0.0.1:4318`. Gunakan `hycli dashboard --no-open` untuk menampilkan alamat tanpa membuka peramban, atau `--port 4320` untuk memilih port lain. Mesin situs web dan dasbor disertakan dalam berkas eksekusi yang sama.

1. Buka **Koneksi AI** dan hubungkan penyedia.
2. Tambahkan alamat situs web dan pilih AI yang akan menyiapkannya.
3. Tinjau tindakan yang tersedia. Jalankan dari dasbor atau buka **Agen pemrograman** untuk menyalin konfigurasi MCP satu kali.

Untuk situs yang memerlukan masuk akun, hubungkan akunnya melalui **Akun**. Satu situs dapat memiliki beberapa akun tersimpan; Anda memilih akun yang digunakan alatnya.

<a id="what-your-ai-can-do"></a>
## Apa yang dapat dilakukan AI Anda

Hycli mengubah operasi situs web yang didukung menjadi alat bernama dengan tipe data yang jelas. Deskripsi yang ditulis AI menjelaskan tujuan setiap tindakan, informasi yang diperlukan, dan hasil yang dikembalikan.

Tiga agen AI independen menangani pembacaan dan pencarian, alur kerja yang berguna, serta bukti akun. Dasbor menampilkan tahap yang selesai, status tiap agen, waktu yang berlalu, dan catatan kerja dalam bahasa sederhana. Pekerjaan CLI dan MCP muncul dalam riwayat aktivitas yang sama.

<p align="center">
  <img src="../../docs/images/preparation.gif" alt="Tiga pekerja. Seekor burung kecil yang sangat sibuk." width="100%" />
</p>
<p align="center"><sub>Tiga pekerja. Seekor burung kecil yang sangat sibuk.</sub></p>

Persiapan mengikuti informasi yang benar-benar tersedia dari situs web: dokumentasi tertaut, skema API, JavaScript yang dipublikasikan, dan struktur permintaan yang diamati ekstensi pendamping peramban. Hycli dapat melakukan pembacaan terbatas untuk memeriksa operasi. Ia tidak membuat catatan uji, mengedit konten, menghapus objek, menebak daftar besar endpoint, atau melakukan fuzzing pada server aktif.

| Tindakan | Perilaku |
| --- | --- |
| Membaca atau mencari | Berjalan secara mandiri jika operasi didukung bukti dan diklasifikasikan sebagai pembacaan. |
| Membuat, mengirim, mengedit, atau menghapus | Menampilkan situs web, akun, tindakan, dan nilai masukan yang telah ditentukan untuk disetujui pengguna. |
| Dampak tidak jelas | Memerlukan peninjauan sebelum permintaan dikirim. |
| Autentikasi, tantangan verifikasi, atau batas permintaan | Menjeda permintaan yang terdampak agar Anda dapat menghubungkan ulang atau menunggu. |

Persetujuan berlaku untuk tepat satu permintaan, kedaluwarsa setelah lima menit, dan hanya dapat digunakan sekali. Perubahan masukan, akun, informasi login tersimpan, atau definisi alat terpasang membatalkannya. Eksekusi melalui CLI, MCP, dan dasbor menggunakan batas yang sama. Operasi tulis tidak pernah dicoba ulang secara otomatis.

Dukungan bergantung pada situs web. Halaman tanpa dokumentasi yang dapat digunakan atau operasi yang telah diamati mungkin memerlukan peramban yang sudah masuk akun, SiteSpec yang disediakan, atau persiapan tambahan. Hycli melaporkan apa yang dapat didukung alih-alih mengklaim bahwa setiap situs memiliki API siap pakai.

### Cakupan dukungan

Mendukung OpenAPI JSON atau YAML, REST, GraphQL, serta isi permintaan JSON atau formulir yang dienkode URL. Rekaman, tautan, dan halaman berikutnya dapat diekstrak dari HTML memakai selektor yang diamati; halaman GET yang dirender Chromium lokal juga dapat dibaca. Jika pembacaan gagal saat persiapan, AI memeriksa bukti, memperbaiki definisi, dan memverifikasi ulang.

Urutan klik browser bebas, unggahan multipart, dan unduhan biner belum didukung. Paket tidak menyertakan akun atau CLI yang dibuat sebelumnya.

[Kontrak operasi](../SITESPEC.md) · [Verifikasi paket](../RELEASING.md)

<a id="ai-connections"></a>
## Koneksi AI

| Koneksi | Cara masuk |
| --- | --- |
| ChatGPT · API key | Kunci API OpenAI Anda |
| Claude | Kunci API Anthropic Anda |
| xAI | Kunci API xAI Anda |
| Z.ai | Kunci API Z.ai Anda |
| Z.ai Coding Plan | Kunci API Z.ai Coding Plan Anda |
| ChatGPT · login | Masuk ChatGPT yang dikelola oleh Codex app-server terpasang |

Pilih model di dasbor, termasuk model yang tersedia khusus untuk akun Anda. Pemeriksaan koneksi memverifikasi penyedia dan model terpilih. Penyedia API menagihkan penggunaan ke akun Anda di penyedia tersebut.

Koneksi Codex menggunakan protokol app-server resminya. Hycli tidak membaca atau menyalin token OAuth Codex. Inferensi berjalan dalam thread sementara dengan eksekusi berkas, shell, peramban, aplikasi, dan MCP dinonaktifkan. Persiapan situs web dilakukan oleh alat pembacaan yang dikendalikan Hycli.

<a id="use-hycli-with-your-ai"></a>
## Gunakan Hycli dengan AI Anda

**Agen pemrograman → Lihat pengaturan koneksi**

Koneksi MCP umum mencakup persiapan situs dan eksekusi tindakan. Hubungkan agen sekali agar dapat menyiapkan alat yang belum ada, mengikuti progres, dan menggunakan tindakan baru.

```sh
hycli mcp
```

Gunakan `--sites-only` untuk menampilkan tindakan yang sudah terpasang saja. Koneksi berikut dibatasi ke satu situs.

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

Jika `hycli` tidak ada di PATH agen, gunakan lokasi executable yang ditampilkan dasbor.

Alur yang sama tersedia melalui CLI. Ganti URL contoh, `SITE`, dan `ACTION` dengan situs Anda serta nama yang dikembalikan `describe`.

```sh
hycli prepare https://your-website.example --intent "Temukan referensi yang disimpan"
hycli request SITE "Add draft editing and publishing with full post content"
hycli describe
hycli describe SITE ACTION
hycli SITE ACTION --help
hycli run SITE ACTION --arg query="design systems"
hycli jobs show JOB_ID --watch
```

Di MCP, kirim URL dan `intent` ke `hycli_prepare`, lalu ikuti tugas dengan `hycli_job` atau `hycli_result`. `hycli_run` dapat menjalankan tindakan baru sebelum klien menyegarkan daftar alat. Cari ID internal melalui tindakan daftar atau pencarian terkait terlebih dahulu.

Tindakan perubahan mengembalikan tanda terima untuk ditinjau di dasbor. Ikuti hasilnya tanpa mengulangi permintaan. Pembacaan gagal atau respons yang tidak sesuai juga menghasilkan kegagalan di CLI.

[Panduan agen](../../agent/AGENTS.md) · [Skill Hycli](../../skills/hycli/SKILL.md)

<a id="browser-sign-in"></a>
## Masuk melalui peramban

Persiapan situs web dimulai dengan memeriksa sistem operasi saat ini, peramban yang berjalan dan terdaftar, serta profil yang tersedia. AI memilih dari kemampuan tersebut untuk membaca dokumentasi, menampilkan halaman, mengimpor sesi situs, dan memverifikasi koneksi. Jalur peramban dan profil tetap berada di mesin lokal; definisi situs web yang dihasilkan tidak bergantung pada jalur instalasi pengembang.

Cookie dan penyimpanan asal dari Chrome, Chromium, Edge, Brave, dan Firefox dapat diimpor jika dapat diakses. Hanya sesi situs web yang diminta yang masuk ke brankas lokal Hycli. Hycli otomatis memilih satu identitas yang terverifikasi; profil milik identitas yang sama dihitung sebagai satu pilihan akun. Pilihan akun yang sudah ada dipertahankan; identitas terverifikasi yang berbeda memerlukan pilihan.

Di **Akun → Hubungkan akun**, **Temukan login saya** mengulangi pencarian tersebut. Jika tidak ada sesi yang dapat digunakan, **Buka halaman login situs web** membuka situs di browser bawaan Anda. Hycli memeriksa lagi selama dialog terbuka dan melanjutkan persiapan hanya setelah akun yang dipilih terverifikasi. Membuka tab saja bukan bukti login berhasil.

Sesi yang dilindungi sistem operasi, terikat pada browser, bersifat privat, atau berada dalam container mungkin memerlukan pendamping browser. Pilih tab pendamping dan buat kode koneksi, lalu buka pendamping dalam profil yang sudah login, masukkan kode, dan berikan akses ke situs tersebut. Impor langsung tidak melewati perlindungan Windows App-Bound atau melemahkan profil browser.

- **Chrome dan Edge:** ekstrak paket dan gunakan **Muat yang belum dipaketkan** pada halaman ekstensi peramban.
- **Firefox:** gunakan paket Firefox dan **Muat Pengaya Sementara** di `about:debugging`. Ekstensi sementara dihapus saat Firefox dimulai ulang. Rilis ini belum menyertakan distribusi bertanda tangan melalui toko ekstensi.
- **Berkas cookie:** impor berkas cookie JSON dan Netscape tersedia sebagai alternatif. Impor berkas langsung di dasbor; jangan tempelkan isinya ke percakapan AI.

Peramban mengirim data sesi langsung ke brankas kredensial lokal. Model menerima label akun, nama dan struktur kunci lokal, serta hasil alat tanpa nilai kredensial. Model dapat menyusun resep koneksi yang merujuk pada nilai lokal tersebut tanpa menerimanya. Cakupan, jalur, masa berlaku, dan informasi partisi cookie yang didukung tetap dihormati; header autentikasi dibatasi pada asalnya semula.

Identitas akun berasal dari respons akun saat ini yang benar-benar dikirim situs web. Nama profil peramban tidak dianggap sebagai identitas situs web yang telah diverifikasi. Jika akun belum diidentifikasi, Hycli menyatakannya. Setelah terhubung, penjelajahan biasa dapat menyediakan respons akun dan struktur permintaan yang tersedia tanpa mengungkap nilai permintaan, header, atau isi respons kepada model persiapan.

Situs mungkin memerlukan kredensial API terpisah yang didokumentasikan atau kemampuan peramban yang tidak tersedia secara lokal. Persiapan mencatat hasil koneksi dan pembacaan yang sebenarnya. Mengambil halaman beranda atau menerima halaman login dari API tidak berarti integrasi sudah siap. [Panduan penyiapan peramban](../../browser-companion/guide.html) menjelaskan alur pendamping dan batasannya.

<a id="languages"></a>
## Bahasa

Dasbor mendukung 20 bahasa yang ditautkan di atas, termasuk bahasa Arab yang ditulis dari kanan ke kiri. Bahasa awalnya adalah bahasa Inggris. Pilihan Anda disimpan di perangkat ini, dan deskripsi AI dapat disiapkan dalam bahasa terpilih.

<a id="local-data"></a>
## Data lokal dan batas permintaan

Definisi situs web, metadata akun, dan aktivitas disimpan di direktori data lokal. Atur `HYCLI_DATA_DIR` untuk memilih lokasi lain. Gunakan **Pengaturan** untuk melihat lokasi aktif dan perlindungan kredensial.

Jika tersedia, brankas kredensial menggunakan kunci enkripsi yang dilindungi keyring sistem operasi. Jika keyring tidak tersedia, sistem secara jelas melaporkan bahwa penyimpanan hanya dilindungi izin berkas; direktori privat dan berkas kredensial dibatasi untuk pengguna sistem operasi saat ini. Brankas terenkripsi yang sudah ada tidak diganti diam-diam saat tidak dapat dibuka.

Permintaan diproses berurutan dan diatur jedanya per situs web dan akun, dengan kuota tambahan untuk seluruh situs. Hycli mematuhi `Retry-After`, membatasi ukuran respons dan percobaan ulang pembacaan, serta menjeda saat autentikasi gagal atau muncul tantangan verifikasi situs. Selama persiapan situs web, Hycli tidak menggilir akun, memalsukan sidik jari, atau melewati tantangan; perilaku diagnostik mesin dapat berbeda. Langkah ini mengurangi beban yang dapat dihindari, tetapi tidak menjamin situs tidak akan membatasi akun.

Alamat situs web jaringan privat dan lokal dinonaktifkan secara bawaan. Untuk situs yang dihosting sendiri dan tepercaya, jalankan dasbor atau server MCP dengan `--allow-local` secara eksplisit.

<a id="contributing"></a>
## Pengembangan dan verifikasi

```sh
npm --prefix dashboard ci
npm --prefix dashboard run check
npm --prefix dashboard run build
cargo test --all-targets --locked
cargo build --locked --bin hycli --example dashboard_fixture
node scripts/test-e2e.mjs --full
```

Pengujian menggunakan situs web lokal dan respons penyedia sintetis. Cakupannya meliputi pengikatan persetujuan dan pencegahan pemakaian ulang, penghapusan rahasia, pengalihan, batas laju, larangan mencoba ulang penulisan, perlindungan sesi peramban, dan kontrak respons penyedia. Jangan pernah memakai akun nyata untuk membuat, mengubah, atau menghapus konten hanya demi menguji Hycli.

Tangkapan layar dan GIF menggunakan data contoh yang terisolasi. Lihat [sumber visual](../images/README.md).

## Lisensi dan tujuan penggunaan

Hycli dilisensikan di bawah [Apache-2.0](../../LICENSE).

Hycli ditujukan untuk membuat dan menggunakan alat baris perintah yang mempermudah penggunaan situs web. Gunakan dengan situs web dan akun yang boleh Anda akses.

Perangkat lunak ini disediakan apa adanya, tanpa jaminan. Sejauh diizinkan hukum, penulis dan kontributor tidak bertanggung jawab atas kerugian atau masalah lain yang timbul dari penggunaannya. Anda bertanggung jawab atas cara Anda menggunakannya.

Sejauh diizinkan oleh hukum, penulis dan kontributor tidak bertanggung jawab atas pemblokiran, penangguhan, atau pembatasan akun akibat penggunaan Hycli. Jangan gunakan Hycli untuk peretasan, akses tanpa izin, atau serangan.
