import type { APIRoute } from 'astro';
import { api, ApiError } from '../../../../lib/api';
import { errorUrl, redirectTo, safeId, okUrl } from '../../../../lib/http';

export const POST: APIRoute = async ({ params, locals }) => {
  const id = safeId(params.id ?? null);
  if (!id) return redirectTo('/dashboard');

  try {
    await api(`/api/projects/${id}`, { method: 'DELETE', token: locals.token });
    return redirectTo(okUrl('/dashboard', 'Proyek dihapus'));
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal menghapus proyek';
    return redirectTo(errorUrl(`/projects/${id}`, message));
  }
};
