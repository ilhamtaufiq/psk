// Lampiran pada komentar dan balasan: pilih berkas, seret ke formulir, atau tempel gambar dari clipboard.

const MAX_FILES = 5;
const MAX_BYTES = 10 * 1024 * 1024;

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function setupForm(form: HTMLFormElement) {
  const input = form.querySelector<HTMLInputElement>('[data-attach-input]');
  const list = form.querySelector<HTMLUListElement>('[data-attach-list]');
  const note = form.querySelector<HTMLElement>('[data-attach-note]');
  const textarea = form.querySelector<HTMLTextAreaElement>('textarea[name="body"]');

  let picked: File[] = [];

  const say = (message: string) => {
    if (!note) return;
    note.textContent = message;
    note.hidden = !message;
  };

  // Daftar disimpan sendiri, lalu dipasang ulang ke input file agar selalu sinkron saat dikirim.
  const sync = () => {
    if (input) {
      const dt = new DataTransfer();
      picked.forEach((f) => dt.items.add(f));
      input.files = dt.files;
    }
    render();
  };

  const render = () => {
    if (!list) return;
    list.replaceChildren(
      ...picked.map((file, index) => {
        const li = document.createElement('li');
        li.className = 'attach-item';
        if (file.type.startsWith('image/')) {
          const img = document.createElement('img');
          img.src = URL.createObjectURL(file);
          img.alt = '';
          img.className = 'attach-thumb';
          li.appendChild(img);
        }
        const name = document.createElement('span');
        name.className = 'attach-name';
        name.textContent = file.name;
        const size = document.createElement('span');
        size.className = 'muted small';
        size.textContent = formatSize(file.size);
        const remove = document.createElement('button');
        remove.type = 'button';
        remove.className = 'icon-btn sm';
        remove.dataset.remove = String(index);
        remove.setAttribute('aria-label', `Hapus lampiran ${file.name}`);
        remove.textContent = '×';
        li.append(name, size, remove);
        return li;
      }),
    );
    list.hidden = picked.length === 0;
  };

  const add = (files: File[]) => {
    const rejected: string[] = [];
    for (const file of files) {
      if (picked.length >= MAX_FILES) {
        say(`Maksimal ${MAX_FILES} lampiran per komentar.`);
        break;
      }
      if (file.size > MAX_BYTES) {
        rejected.push(file.name);
        continue;
      }
      if (file.size === 0) continue;
      picked.push(file);
    }
    if (rejected.length) say(`Melebihi 10 MB: ${rejected.join(', ')}`);
    else if (picked.length < MAX_FILES) say('');
    sync();
  };

  input?.addEventListener('change', () => add(Array.from(input.files ?? [])));

  list?.addEventListener('click', (event) => {
    const button = (event.target as HTMLElement).closest<HTMLButtonElement>('button[data-remove]');
    if (!button) return;
    picked.splice(Number(button.dataset.remove), 1);
    say('');
    sync();
  });

  // Gambar atau berkas yang ditempel ikut dilampirkan; teks tetap ditempel biasa.
  textarea?.addEventListener('paste', (event) => {
    const files = Array.from(event.clipboardData?.files ?? []);
    if (files.length === 0) return;
    event.preventDefault();
    add(files);
  });

  // Seret berkas ke formulir.
  form.addEventListener('dragover', (event) => {
    if (event.dataTransfer?.types.includes('Files')) event.preventDefault();
  });
  form.addEventListener('drop', (event) => {
    const files = Array.from(event.dataTransfer?.files ?? []);
    if (files.length === 0) return;
    event.preventDefault();
    add(files);
  });

  // Komentar harus berisi teks atau minimal satu lampiran.
  form.addEventListener('submit', (event) => {
    const hasText = (textarea?.value ?? '').trim().length > 0;
    if (!hasText && picked.length === 0) {
      event.preventDefault();
      say('Tulis komentar atau lampirkan berkas.');
      textarea?.focus();
    }
  });
}

document.querySelectorAll<HTMLFormElement>('form[data-attach-form]').forEach(setupForm);
