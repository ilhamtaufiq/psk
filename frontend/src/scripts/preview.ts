import { previewKind, type PreviewKind } from '../lib/preview';

const MAX_ROWS = 500;
const MAX_COLS = 60;

const dialog = document.getElementById('file-preview') as HTMLDialogElement | null;
const body = document.getElementById('file-preview-body') as HTMLElement | null;
const titleEl = document.getElementById('file-preview-title') as HTMLElement | null;
const downloadEl = document.getElementById('file-preview-download') as HTMLAnchorElement | null;

function setStatus(message: string) {
  if (!body) return;
  const p = document.createElement('p');
  p.className = 'preview-status';
  p.textContent = message;
  body.replaceChildren(p);
}

async function fetchBuffer(id: string): Promise<ArrayBuffer> {
  const res = await fetch(`/api/files/${id}`, { credentials: 'same-origin' });
  if (!res.ok) throw new Error('Gagal mengambil berkas');
  return res.arrayBuffer();
}

async function renderSheet(buffer: ArrayBuffer) {
  if (!body) return;
  const XLSX = await import('xlsx');
  const wb = XLSX.read(new Uint8Array(buffer), { type: 'array' });

  const tabs = document.createElement('div');
  tabs.className = 'sheet-tabs';
  const tableWrap = document.createElement('div');
  tableWrap.className = 'sheet-table-wrap';
  body.replaceChildren(tabs, tableWrap);

  const show = (index: number) => {
    const rows = XLSX.utils.sheet_to_json<unknown[]>(wb.Sheets[wb.SheetNames[index]], {
      header: 1,
      defval: '',
      blankrows: false,
    });
    const table = document.createElement('table');
    table.className = 'sheet-table';
    rows.slice(0, MAX_ROWS).forEach((row, r) => {
      const tr = document.createElement('tr');
      row.slice(0, MAX_COLS).forEach((cell) => {
        const el = document.createElement(r === 0 ? 'th' : 'td');
        // textContent: isi sel tidak pernah ditafsirkan sebagai HTML.
        el.textContent = cell === null || cell === undefined ? '' : String(cell);
        tr.append(el);
      });
      table.append(tr);
    });
    const nodes: Node[] = [table];
    if (rows.length > MAX_ROWS) {
      const note = document.createElement('p');
      note.className = 'preview-note';
      note.textContent = `Menampilkan ${MAX_ROWS} dari ${rows.length} baris. Unduh berkas untuk melihat semuanya.`;
      nodes.push(note);
    }
    tableWrap.replaceChildren(...nodes);
    tabs.querySelectorAll('button').forEach((b, i) => b.setAttribute('aria-pressed', String(i === index)));
  };

  wb.SheetNames.forEach((name, i) => {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'sheet-tab';
    btn.textContent = name || `Lembar ${i + 1}`;
    btn.addEventListener('click', () => show(i));
    tabs.append(btn);
  });
  show(0);
}

async function renderKind(kind: PreviewKind, id: string, name: string) {
  if (!body) return;
  switch (kind) {
    case 'image': {
      const img = document.createElement('img');
      img.className = 'preview-img';
      img.alt = name;
      img.src = `/api/files/${id}?inline=1`;
      body.replaceChildren(img);
      return;
    }
    case 'pdf': {
      const frame = document.createElement('iframe');
      frame.className = 'preview-frame';
      frame.title = name;
      frame.src = `/api/files/${id}?inline=1`;
      body.replaceChildren(frame);
      return;
    }
    case 'docx': {
      const buffer = await fetchBuffer(id);
      const { renderAsync } = await import('docx-preview');
      const holder = document.createElement('div');
      holder.className = 'preview-docx';
      body.replaceChildren(holder);
      await renderAsync(new Blob([buffer]), holder, undefined, { inWrapper: true });
      return;
    }
    case 'sheet':
      return renderSheet(await fetchBuffer(id));
    case 'pptx': {
      const buffer = await fetchBuffer(id);
      const { init } = await import('pptx-preview');
      const holder = document.createElement('div');
      holder.className = 'preview-pptx';
      body.replaceChildren(holder);
      const width = Math.min(holder.clientWidth || 960, 960);
      const viewer = init(holder, { width, height: Math.round((width * 9) / 16) });
      await viewer.preview(buffer);
      return;
    }
    case 'text': {
      const buffer = await fetchBuffer(id);
      const pre = document.createElement('pre');
      pre.className = 'preview-text';
      pre.textContent = new TextDecoder('utf-8').decode(buffer.slice(0, 2_000_000));
      body.replaceChildren(pre);
      return;
    }
    default:
      setStatus('Pratinjau belum tersedia untuk jenis berkas ini. Silakan unduh berkasnya.');
  }
}

async function openPreview(id: string, name: string) {
  if (!dialog || !body || !titleEl || !downloadEl) return;
  titleEl.textContent = name;
  downloadEl.href = `/api/files/${id}`;
  setStatus('Memuat pratinjau…');
  if (!dialog.open) dialog.showModal();

  try {
    await renderKind(previewKind(name), id, name);
  } catch (err) {
    console.error(err);
    setStatus('Pratinjau gagal dimuat. Coba unduh berkasnya.');
  }
}

document.addEventListener('click', (event) => {
  const trigger = (event.target as HTMLElement).closest<HTMLElement>('[data-preview]');
  if (!trigger) return;
  event.preventDefault();
  void openPreview(trigger.dataset.id ?? '', trigger.dataset.name ?? '');
});

dialog?.addEventListener('close', () => {
  // Hentikan iframe/viewer agar tidak terus memuat setelah ditutup.
  body?.replaceChildren();
});

dialog?.addEventListener('click', (event) => {
  // Klik di area gelap di luar panel menutup dialog.
  if (event.target === dialog) dialog.close();
});

document.querySelectorAll<HTMLElement>('[data-preview-close]').forEach((el) =>
  el.addEventListener('click', () => dialog?.close()),
);
