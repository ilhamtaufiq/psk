// Klien untuk API Rust. Hanya dipanggil dari sisi server (SSR / endpoint Astro),
// sehingga token sesi tidak pernah dikirim ke peramban selain lewat cookie httpOnly.
export const API_URL = process.env.API_URL ?? 'http://127.0.0.1:8080';

export class ApiError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
  }
}

export type ProjectStatus = 'planning' | 'active' | 'on_hold' | 'completed';

export const STATUS_LABELS: Record<ProjectStatus, string> = {
  planning: 'Perencanaan',
  active: 'Sedang berjalan',
  on_hold: 'Ditunda',
  completed: 'Selesai',
};

export type ProjectType = 'administrasi' | 'konstruksi' | 'lainnya' | 'operasional' | 'pemasaran' | 'riset' | 'teknologi';

export const PROJECT_TYPE_LABELS: Record<ProjectType, string> = {
  administrasi: 'Administrasi',
  konstruksi: 'Konstruksi',
  lainnya: 'Lainnya',
  operasional: 'Operasional',
  pemasaran: 'Pemasaran',
  riset: 'Riset & Pengembangan',
  teknologi: 'Teknologi / IT',
};

export const PROJECT_SORTS: Record<string, string> = {
  updated: 'Terakhir diubah',
  created: 'Terbaru dibuat',
  name: 'Nama (A–Z)',
  due: 'Tenggat terdekat',
  type: 'Jenis proyek',
  status: 'Status',
};

export interface User {
  id: string;
  name: string;
  email: string;
  created_at: string;
}

export interface Project {
  id: string;
  name: string;
  description: string;
  status: ProjectStatus;
  project_type: ProjectType;
  start_date: string | null;
  due_date: string | null;
  created_at: string;
  updated_at: string;
  file_count: number;
  req_total: number;
  req_done: number;
}

export interface Requirement {
  id: string;
  project_id: string;
  title: string;
  created_at: string;
  file_count: number;
}

export interface ProjectFile {
  id: string;
  project_id: string;
  requirement_id: string | null;
  original_name: string;
  mime_type: string;
  size_bytes: number;
  uploaded_by_name: string;
  created_at: string;
}

export interface ProjectInput {
  name: string;
  description?: string;
  status?: ProjectStatus;
  project_type?: ProjectType;
  start_date?: string | null;
  due_date?: string | null;
}

interface RequestOptions {
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE';
  token?: string | null;
  json?: unknown;
  body?: FormData;
}

function buildInit(opts: RequestOptions): RequestInit {
  const headers = new Headers();
  if (opts.token) headers.set('authorization', `Bearer ${opts.token}`);

  let body: BodyInit | undefined;
  if (opts.json !== undefined) {
    headers.set('content-type', 'application/json');
    body = JSON.stringify(opts.json);
  } else if (opts.body) {
    body = opts.body; // fetch mengatur content-type multipart beserta boundary-nya
  }
  return { method: opts.method ?? 'GET', headers, body };
}

async function errorFrom(res: Response): Promise<ApiError> {
  let message = `Permintaan gagal (${res.status})`;
  try {
    const data = (await res.json()) as { error?: string };
    if (data.error) message = data.error;
  } catch {
    /* body bukan JSON */
  }
  return new ApiError(res.status, message);
}

export async function api<T>(path: string, opts: RequestOptions = {}): Promise<T> {
  const res = await fetch(API_URL + path, buildInit(opts));
  if (!res.ok) throw await errorFrom(res);
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

/** Mengembalikan Response mentah (untuk streaming unduhan berkas). */
export async function apiRaw(path: string, opts: RequestOptions = {}): Promise<Response> {
  const res = await fetch(API_URL + path, buildInit(opts));
  if (!res.ok) throw await errorFrom(res);
  return res;
}

export interface Bucket {
  key: string;
  count: number;
}

export interface DashboardData {
  today: string;
  total_projects: number;
  by_status: Bucket[];
  by_type: Bucket[];
  overdue: number;
  due_soon: Array<{ id: string; name: string; status: ProjectStatus; due_date: string }>;
  total_files: number;
  total_bytes: number;
  requirements_total: number;
  requirements_done: number;
  uploads_by_month: Bucket[];
  recent: Array<{ id: string; name: string; status: ProjectStatus; updated_at: string }>;
}
