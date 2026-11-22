import './comments';

// Interaksi UI bersama: toast, dialog konfirmasi, progress navigasi, pencarian daftar,
// validasi inline, dan unggah berkas dengan progress bar.

// ---------- Toast ----------
const toast = document.querySelector<HTMLElement>('.toast');
let toastTimer: number | undefined;

export function showToast(message: string, kind: 'ok' | 'error' = 'ok') {
  if (!toast) return;
  toast.textContent = message;
  toast.dataset.kind = kind;
  toast.hidden = false;
  toast.classList.add('show');
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => toast.classList.remove('show'), 3200);
}

// Pesan sukses dari redirect (?ok=...) ditampilkan sekali lalu dibersihkan dari URL.
{
  const url = new URL(location.href);
  const ok = url.searchParams.get('ok');
  if (ok) {
    showToast(ok, 'ok');
    url.searchParams.delete('ok');
    history.replaceState(null, '', url.pathname + url.search + url.hash);
  }
}

// ---------- Dialog konfirmasi (menggantikan confirm() bawaan) ----------
const dialog = document.querySelector<HTMLDialogElement>('.confirm-dialog');
const dialogMessage = document.querySelector<HTMLElement>('.confirm-msg');
let pendingForm: HTMLFormElement | null = null;

document.addEventListener(
  'submit',
  (event) => {
    const form = event.target;
    if (!(form instanceof HTMLFormElement)) return;
    const message = form.dataset.confirm;
    if (!message || form.dataset.confirmed === '1' || !dialog || !dialogMessage) return;
    event.preventDefault();
    pendingForm = form;
    dialogMessage.textContent = message;
    dialog.showModal();
  },
  true,
);

dialog?.addEventListener('close', () => {
  const form = pendingForm;
  pendingForm = null;
  if (dialog.returnValue === 'ok' && form) {
    form.dataset.confirmed = '1';
    form.requestSubmit();
  }
});

// ---------- Progress navigasi (bar tipis di atas) ----------
const progress = document.querySelector<HTMLElement>('.nav-progress');
const startProgress = () => progress?.classList.add('running');

document.addEventListener('click', (event) => {
  if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
  const link = (event.target as Element).closest<HTMLAnchorElement>('a[href]');
  if (!link || link.target || link.hasAttribute('download')) return;
  const url = new URL(link.href, location.href);
  if (url.origin !== location.origin) return;
  if (url.pathname === location.pathname && url.search === location.search) return; // hanya ganti anchor
  startProgress();
});

document.addEventListener('submit', (event) => {
  if (!event.defaultPrevented) startProgress();
});

window.addEventListener('pageshow', () => progress?.classList.remove('running'));

// ---------- Pencarian daftar (client-side) ----------
document.addEventListener('input', (event) => {
  const input = event.target as HTMLInputElement;
  if (!input.matches('input[data-filter-list]')) return;
  const list = document.querySelector<HTMLElement>(input.dataset.filterList ?? '');
  if (!list) return;
  const query = input.value.trim().toLowerCase();
  let shown = 0;
  list.querySelectorAll<HTMLElement>(':scope > li').forEach((item) => {
    const match = !query || (item.textContent ?? '').toLowerCase().includes(query);
    item.hidden = !match;
    if (match) shown++;
  });
  const empty = document.querySelector<HTMLElement>('[data-filter-empty]');
  if (empty) empty.hidden = !query || shown > 0;
});

// ---------- Validasi inline di bawah field ----------
type Field = HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement;

function messageFor(field: Field): string {
  const v = field.validity;
  if (v.valueMissing) return 'Kolom ini wajib diisi.';
  if (v.typeMismatch) {
    const type = (field as HTMLInputElement).type;
    if (type === 'email') return 'Masukkan alamat email yang valid.';
    if (type === 'url') return 'Masukkan tautan yang valid, diawali https://.';
    return 'Format isian tidak sesuai.';
  }
  const text = field as HTMLInputElement;
  if (v.tooShort) return `Minimal ${text.minLength} karakter.`;
  if (v.tooLong) return `Maksimal ${text.maxLength} karakter.`;
  if (v.patternMismatch) return 'Format isian tidak sesuai.';
  if (v.rangeUnderflow || v.rangeOverflow || v.stepMismatch) return 'Nilai di luar batas yang diizinkan.';
  if (v.badInput) return 'Isian tidak valid.';
  return 'Periksa kembali isian ini.';
}

function clearFieldError(field: Field) {
  const next = field.nextElementSibling;
  if (next?.classList.contains('field-error')) next.remove();
  field.removeAttribute('aria-invalid');
}

document.addEventListener(
  'invalid',
  (event) => {
    const field = event.target as Field;
    if (!field.form || !field.closest('.card, .form, .toolbar, .field-form, .drive-form')) return;
    event.preventDefault();
    clearFieldError(field);
    const error = document.createElement('span');
    error.className = 'field-error';
    error.textContent = messageFor(field);
    field.insertAdjacentElement('afterend', error);
    field.setAttribute('aria-invalid', 'true');
    field.focus();
  },
  true,
);

for (const type of ['input', 'change']) {
  document.addEventListener(type, (event) => {
    const field = event.target as Field;
    if (field.hasAttribute('aria-invalid')) clearFieldError(field);
  });
}

// ---------- Unggah berkas dengan progress bar ----------
const FILE_UPLOAD = /\/projects\/[0-9a-f-]+\/files$/;

document.addEventListener('change', (event) => {
  const input = event.target as HTMLInputElement;
  if (!input.matches('input[data-autoupload]') || !input.files?.length || !input.form) return;
  const form = input.form;
  const file = input.files[0];
  const action = form.getAttribute('action') ?? '';

  // Unggahan dari komputer dikirim lewat XHR agar progresnya terlihat.
  if (!FILE_UPLOAD.test(action)) {
    form.requestSubmit();
    return;
  }

  const title = input.closest('label')?.querySelector('.dz-title');
  if (title) title.textContent = 'Mengunggah…';
  const data = new FormData(form); // ambil dulu: field yang nonaktif tidak ikut terkirim
  input.disabled = true;

  const bar = document.createElement('div');
  bar.className = 'upload-progress';
  bar.setAttribute('role', 'progressbar');
  bar.setAttribute('aria-label', `Mengunggah ${file.name}`);
  bar.setAttribute('aria-valuemin', '0');
  bar.setAttribute('aria-valuemax', '100');
  bar.innerHTML = `<span></span><small></small>`;
  bar.querySelector('small')!.textContent = `${file.name} · 0%`;
  form.appendChild(bar);

  const xhr = new XMLHttpRequest();
  xhr.open('POST', action);
  xhr.upload.onprogress = (e) => {
    if (!e.lengthComputable) return;
    const pct = Math.round((e.loaded / e.total) * 100);
    bar.querySelector<HTMLElement>('span')!.style.width = `${pct}%`;
    bar.setAttribute('aria-valuenow', String(pct));
    bar.querySelector('small')!.textContent = `${file.name} · ${pct}%`;
  };
  xhr.onload = () => {
    // Server membalas redirect (ok atau error); browser mengikutinya ke halaman tujuan.
    location.href = xhr.responseURL || location.href;
  };
  xhr.onerror = () => {
    bar.remove();
    input.disabled = false;
    if (title) title.textContent = 'Ketuk untuk memilih berkas';
    showToast('Gagal mengunggah. Periksa koneksi lalu coba lagi.', 'error');
  };
  xhr.send(data);
});

// ---------- Tema: sistem → terang → gelap ----------
const THEME_KEY = 'psk-theme';
const THEME_MODES = ['system', 'light', 'dark'];
const THEME_LABELS: Record<string, string> = { system: 'sistem', light: 'terang', dark: 'gelap' };
const themeBtn = document.querySelector<HTMLButtonElement>('[data-theme-toggle]');

function currentTheme(): string {
  try {
    const saved = localStorage.getItem(THEME_KEY);
    return saved === 'light' || saved === 'dark' ? saved : 'system';
  } catch (e) {
    return 'system';
  }
}

function applyTheme(mode: string) {
  const root = document.documentElement;
  if (mode === 'system') root.removeAttribute('data-theme');
  else root.setAttribute('data-theme', mode);
  try {
    if (mode === 'system') localStorage.removeItem(THEME_KEY);
    else localStorage.setItem(THEME_KEY, mode);
  } catch (e) {
    /* penyimpanan diblokir: tema tetap berlaku untuk halaman ini */
  }
  if (themeBtn) {
    const label = THEME_LABELS[mode];
    themeBtn.dataset.mode = mode;
    themeBtn.setAttribute('aria-label', `Tema: ${label}. Klik untuk ganti.`);
    themeBtn.title = `Tema: ${label}`;
  }
}

function cycleTheme() {
  const next = THEME_MODES[(THEME_MODES.indexOf(currentTheme()) + 1) % THEME_MODES.length];
  applyTheme(next);
  showToast(`Tema ${THEME_LABELS[next]}`, 'ok');
}

applyTheme(currentTheme());
themeBtn?.addEventListener('click', cycleTheme);

// ---------- Palet perintah (Ctrl/⌘+K, atau "/") ----------
type PaletteItem = { label: string; hint: string; href?: string; action?: 'theme' };

const palette = document.querySelector<HTMLDialogElement>('dialog.palette');
const paletteInput = palette?.querySelector<HTMLInputElement>('[data-palette-input]') ?? null;
const paletteList = palette?.querySelector<HTMLUListElement>('[data-palette-list]') ?? null;

const PAGES: PaletteItem[] = [
  { label: 'Dashboard', hint: 'Halaman', href: '/dashboard' },
  { label: 'Daftar project', hint: 'Halaman', href: '/projects' },
  { label: 'Project baru', hint: 'Halaman', href: '/projects/new' },
  { label: 'Laporan', hint: 'Halaman', href: '/laporan' },
  { label: 'Jenis project', hint: 'Halaman', href: '/jenis' },
  { label: 'Ganti tema', hint: 'Aksi', action: 'theme' },
];

let projectItems: PaletteItem[] | null = null;
let projectsLoading: Promise<void> | null = null;
let visibleItems: PaletteItem[] = [];
let activeIndex = 0;

function loadProjects(): Promise<void> {
  if (projectItems) return Promise.resolve();
  if (!projectsLoading) {
    projectsLoading = fetch('/api/palette', { headers: { accept: 'application/json' }, credentials: 'same-origin' })
      .then((res) => (res.ok ? res.json() : []))
      .then((list: { id: string; name: string; project_type_label: string; status_label: string }[]) => {
        projectItems = list.map((p) => ({
          label: p.name,
          hint: `${p.project_type_label} · ${p.status_label}`,
          href: `/projects/${p.id}`,
        }));
      })
      .catch(() => {
        projectItems = [];
      });
  }
  return projectsLoading;
}

function renderPalette() {
  if (!paletteList || !paletteInput) return;
  const query = paletteInput.value.trim().toLowerCase();
  const pool = [...PAGES, ...(projectItems ?? [])];
  visibleItems = (query ? pool.filter((i) => `${i.label} ${i.hint}`.toLowerCase().includes(query)) : pool).slice(0, 30);
  activeIndex = Math.min(activeIndex, Math.max(visibleItems.length - 1, 0));

  const rows: HTMLElement[] = visibleItems.map((item, i) => {
    const li = document.createElement('li');
    li.id = `palette-opt-${i}`;
    li.className = 'palette-item';
    li.setAttribute('role', 'option');
    li.setAttribute('aria-selected', String(i === activeIndex));
    li.dataset.index = String(i);
    const name = document.createElement('span');
    name.className = 'palette-label';
    name.textContent = item.label;
    const hint = document.createElement('span');
    hint.className = 'palette-hint';
    hint.textContent = item.hint;
    li.append(name, hint);
    return li;
  });

  if (!rows.length) {
    const empty = document.createElement('li');
    empty.className = 'palette-empty';
    empty.textContent = projectItems === null && projectsLoading ? 'Memuat project…' : 'Tidak ada hasil.';
    rows.push(empty);
  }
  paletteList.replaceChildren(...rows);
  paletteInput.setAttribute('aria-activedescendant', visibleItems.length ? `palette-opt-${activeIndex}` : '');
  paletteList.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' });
}

function openPalette() {
  if (!palette || !paletteInput || palette.open) return;
  paletteInput.value = '';
  activeIndex = 0;
  palette.showModal();
  renderPalette();
  paletteInput.focus();
  loadProjects().then(() => {
    if (palette.open) renderPalette();
  });
}

function runItem(item: PaletteItem | undefined) {
  if (!item || !palette) return;
  if (item.action === 'theme') {
    palette.close();
    cycleTheme();
    return;
  }
  if (!item.href) return;
  if (new URL(item.href, location.href).pathname === location.pathname) {
    palette.close();
    return;
  }
  startProgress();
  location.href = item.href;
}

paletteInput?.addEventListener('input', () => {
  activeIndex = 0;
  renderPalette();
});

paletteInput?.addEventListener('keydown', (event) => {
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault();
    const count = visibleItems.length;
    if (!count) return;
    activeIndex = (activeIndex + (event.key === 'ArrowDown' ? 1 : -1) + count) % count;
    renderPalette();
  } else if (event.key === 'Enter') {
    event.preventDefault();
    runItem(visibleItems[activeIndex]);
  } else if (event.key === 'Escape' && paletteInput.value) {
    // Esc pertama menghapus kueri (perilaku input pencarian); Esc berikutnya menutup palet.
    event.preventDefault();
    paletteInput.value = '';
    activeIndex = 0;
    renderPalette();
  }
});

paletteList?.addEventListener('click', (event) => {
  const row = (event.target as HTMLElement).closest<HTMLElement>('[data-index]');
  if (row) runItem(visibleItems[Number(row.dataset.index)]);
});

palette?.addEventListener('click', (event) => {
  // Klik di area gelap di luar kotak palet menutupnya.
  if (event.target === palette) palette.close();
});

palette?.addEventListener('close', () => {
  paletteInput?.setAttribute('aria-activedescendant', '');
});

document.addEventListener('click', (event) => {
  if ((event.target as Element).closest('[data-open-palette]')) openPalette();
});

document.addEventListener('keydown', (event) => {
  if (!palette) return;
  const key = event.key.toLowerCase();
  if ((event.ctrlKey || event.metaKey) && key === 'k') {
    event.preventDefault();
    if (palette.open) palette.close();
    else openPalette();
    return;
  }
  if (event.key === '/' && !event.ctrlKey && !event.metaKey && !event.altKey && !palette.open) {
    const target = event.target as HTMLElement;
    const typing = target.matches('input, textarea, select, [contenteditable="true"]');
    if (typing) return;
    event.preventDefault();
    openPalette();
  }
});
