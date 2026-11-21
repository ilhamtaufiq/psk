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
