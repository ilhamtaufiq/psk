import type { APIRoute } from 'astro';
import { api, ApiError, STATUS_LABELS, type Project } from '../../lib/api';

// Daftar ringkas project untuk command palette (Ctrl/⌘+K). Hanya field yang ditampilkan.
export const GET: APIRoute = async ({ locals }) => {
  const headers = { 'content-type': 'application/json', 'cache-control': 'private, no-store' };
  if (!locals.token) return new Response('[]', { status: 401, headers });
  try {
    const list = await api<Project[]>('/api/projects', { token: locals.token });
    const slim = list.slice(0, 200).map((p) => ({
      id: p.id,
      name: p.name,
      project_type_label: p.project_type_label,
      status_label: STATUS_LABELS[p.status] ?? p.status,
    }));
    return new Response(JSON.stringify(slim), { status: 200, headers });
  } catch (err) {
    const status = err instanceof ApiError && err.status === 401 ? 401 : 500;
    return new Response('[]', { status, headers });
  }
};
