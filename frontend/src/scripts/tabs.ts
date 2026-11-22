// Tab halaman detail project. Setiap bagian ber-atribut data-tab; hanya tab aktif yang tampil.
// Tanpa JavaScript semua bagian tampil seperti biasa. Tautan lama (mis. #kelengkapan dari
// redirect) tetap bekerja: hash memilih tab yang memuat bagian tersebut.

const DEFAULT_TAB = 'ringkasan';
let current = DEFAULT_TAB;

function tabFromHash(): { tab: string; target: HTMLElement | null } {
  let id = '';
  try {
    id = decodeURIComponent(location.hash.slice(1));
  } catch {
    id = '';
  }
  if (!id) return { tab: DEFAULT_TAB, target: null };
  const el = document.getElementById(id);
  if (!el) return { tab: DEFAULT_TAB, target: null };
  const panel = el.closest<HTMLElement>('[data-tab]');
  return { tab: panel?.dataset.tab ?? DEFAULT_TAB, target: el };
}

function show(tab: string) {
  current = tab;
  const detail = document.querySelector('.detail');
  if (!detail) return;
  detail.querySelectorAll<HTMLElement>('[data-tab]').forEach((panel) => {
    panel.hidden = panel.dataset.tab !== tab;
  });
  detail.querySelectorAll<HTMLAnchorElement>('[data-tab-link]').forEach((link) => {
    if (link.dataset.tabLink === tab) link.setAttribute('aria-current', 'page');
    else link.removeAttribute('aria-current');
  });
}

function sync(scroll: boolean) {
  const { tab, target } = tabFromHash();
  show(tab);
  if (scroll && target) {
    requestAnimationFrame(() => target.scrollIntoView({ block: 'start' }));
  }
}

if (document.querySelector('.detail')) {
  sync(false);
  window.addEventListener('hashchange', () => sync(true));
  // Setelah data halaman diperbarui langsung (live.ts), tab yang sedang aktif dipertahankan.
  document.addEventListener('psk:detail-updated', () => show(current));
}
