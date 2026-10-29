(() => {
  'use strict';
  const ticket = new URLSearchParams(location.hash.slice(1)).get('ticket');
  history.replaceState(null, '', '/bootstrap');
  // Keep this landing self-contained: never fetch settings or translation assets.
  const messages = {
    en: ['Opening Hycli…', 'Open Hycli again to use this window.'],
    ko: ['Hycli 여는 중…', '이 창을 사용하려면 Hycli를 다시 여세요.'],
    ja: ['Hycli を開いています…', 'このウィンドウを使用するには、Hycli をもう一度開いてください。'],
    'zh-CN': ['正在打开 Hycli…', '请重新打开 Hycli 以使用此窗口。'],
    'zh-TW': ['正在開啟 Hycli…', '請重新開啟 Hycli 以使用此視窗。'],
    es: ['Abriendo Hycli…', 'Vuelve a abrir Hycli para usar esta ventana.'],
    fr: ['Ouverture de Hycli…', 'Rouvrez Hycli pour utiliser cette fenêtre.'],
    de: ['Hycli wird geöffnet…', 'Öffne Hycli erneut, um dieses Fenster zu verwenden.'],
    'pt-BR': ['Abrindo o Hycli…', 'Abra o Hycli novamente para usar esta janela.'],
    id: ['Membuka Hycli…', 'Buka kembali Hycli untuk menggunakan jendela ini.'],
    it: ['Apertura di Hycli…', 'Riapri Hycli per usare questa finestra.'],
    nl: ['Hycli openen…', 'Open Hycli opnieuw om dit venster te gebruiken.'],
    pl: ['Otwieranie Hycli…', 'Otwórz Hycli ponownie, aby użyć tego okna.'],
    tr: ['Hycli açılıyor…', 'Bu pencereyi kullanmak için Hycli uygulamasını yeniden açın.'],
    ru: ['Открытие Hycli…', 'Откройте Hycli ещё раз, чтобы использовать это окно.'],
    uk: ['Відкриття Hycli…', 'Відкрийте Hycli ще раз, щоб скористатися цим вікном.'],
    vi: ['Đang mở Hycli…', 'Mở lại Hycli để sử dụng cửa sổ này.'],
    th: ['กำลังเปิด Hycli…', 'เปิด Hycli อีกครั้งเพื่อใช้หน้าต่างนี้'],
    ar: ['جارٍ فتح Hycli…', 'افتح Hycli مرة أخرى لاستخدام هذه النافذة.'],
    hi: ['Hycli खुल रहा है…', 'इस विंडो का उपयोग करने के लिए Hycli फिर से खोलें।'],
  };
  const language = (navigator.language || '').toLowerCase();
  const primary = language.split('-')[0];
  const locale = Object.keys(messages).find(key => key.toLowerCase() === language)
    || (primary === 'zh' ? (/^zh-(?:hant|tw|hk|mo)(?:-|$)/.test(language) ? 'zh-TW' : 'zh-CN')
      : primary === 'pt' ? 'pt-BR' : Object.hasOwn(messages, primary) ? primary : 'en');
  document.documentElement.lang = locale;
  document.documentElement.dir = locale === 'ar' ? 'rtl' : 'ltr';
  const [opening, reopen] = messages[locale];
  const status = document.getElementById('status');
  status.textContent = opening;
  if (!ticket || !/^[a-f0-9]{64}$/i.test(ticket)) {
    status.textContent = reopen;
    return;
  }
  fetch('/api/bootstrap', {
    method: 'POST', credentials: 'same-origin', cache: 'no-store',
    headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ ticket }),
  }).then(response => {
    if (!response.ok) throw new Error('opening expired');
    location.replace('/');
  }).catch(() => { status.textContent = reopen; });
})();
