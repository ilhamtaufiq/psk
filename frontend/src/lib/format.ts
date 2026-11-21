const DATE_ONLY = /^\d{4}-\d{2}-\d{2}$/;
const DAY_MS = 86_400_000;

/** Tanggal tanpa jam (YYYY-MM-DD) ditampilkan apa adanya di zona UTC, timestamp di WIB. */
export function formatDate(value: string | null | undefined, withTime = false): string {
  if (!value) return '—';
  if (DATE_ONLY.test(value)) {
    return new Date(`${value}T00:00:00Z`).toLocaleDateString('id-ID', {
      day: 'numeric',
      month: 'short',
      year: 'numeric',
      timeZone: 'UTC',
    });
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString('id-ID', {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
    ...(withTime ? { hour: '2-digit', minute: '2-digit' } : {}),
    timeZone: 'Asia/Jakarta',
  });
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** Tanggal hari ini di WIB sebagai YYYY-MM-DD. */
function todayUtcMidnight(): number {
  const parts = new Intl.DateTimeFormat('en-CA', {
    timeZone: 'Asia/Jakarta',
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
  }).format(new Date());
  return Date.parse(`${parts}T00:00:00Z`);
}

const toMidnight = (dateIso: string) => Date.parse(`${dateIso}T00:00:00Z`);

export interface Deadline {
  text: string;
  tone: 'ok' | 'soon' | 'late' | 'done';
}

/** Label tenggat: berapa hari lagi, jatuh tempo hari ini, terlambat, atau selesai. */
export function deadlineOf(due: string | null, status: string): Deadline | null {
  if (!due) return null;
  if (status === 'completed') return { text: 'Selesai', tone: 'done' };
  const days = Math.round((toMidnight(due) - todayUtcMidnight()) / DAY_MS);
  if (days < 0) return { text: `Terlambat ${-days} hari`, tone: 'late' };
  if (days === 0) return { text: 'Jatuh tempo hari ini', tone: 'soon' };
  if (days <= 3) return { text: `${days} hari lagi`, tone: 'soon' };
  return { text: `${days} hari lagi`, tone: 'ok' };
}

/** Persentase waktu yang sudah berjalan antara tanggal mulai dan selesai. */
export function timeProgress(start: string | null, due: string | null, status: string): number | null {
  if (!start || !due) return null;
  if (status === 'completed') return 100;
  const s = toMidnight(start);
  const e = toMidnight(due);
  const now = todayUtcMidnight();
  if (e <= s) return now >= e ? 100 : 0;
  return Math.min(100, Math.max(0, Math.round(((now - s) / (e - s)) * 100)));
}

const KIND_BY_EXT: Record<string, string> = {
  pdf: 'pdf',
  jpg: 'img', jpeg: 'img', png: 'img', gif: 'img', webp: 'img', svg: 'img', heic: 'img',
  doc: 'doc', docx: 'doc', odt: 'doc', txt: 'doc', rtf: 'doc', md: 'doc',
  xls: 'sheet', xlsx: 'sheet', csv: 'sheet', ods: 'sheet',
  ppt: 'slide', pptx: 'slide', key: 'slide',
  zip: 'zip', rar: 'zip', '7z': 'zip', tar: 'zip', gz: 'zip',
};

/** Jenis berkas dari ekstensi, dipakai untuk warna dan label ikon. */
export function fileKind(name: string): { kind: string; label: string } {
  const dot = name.lastIndexOf('.');
  const ext = dot > 0 && dot < name.length - 1 ? name.slice(dot + 1).toLowerCase() : '';
  return {
    kind: KIND_BY_EXT[ext] ?? 'other',
    label: ext ? ext.slice(0, 4).toUpperCase() : 'FILE',
  };
}
