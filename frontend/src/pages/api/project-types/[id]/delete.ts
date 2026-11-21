import type { APIRoute } from 'astro';
import { api, ApiError } from '../../../../lib/api';
import { errorUrl, redirectTo, safeId, okUrl } from '../../../../lib/http';

export const POST: APIRoute = async ({ params, locals }) => {
  const id = safeId(params.id ?? null);
  if (!id) return redirectTo('/jenis');

  try {
    await api(`/api/project-types/${id}`, { method: 'DELETE', token: locals.token });
    return redirectTo(okUrl('/jenis', 'Jenis dihapus'));
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal menghapus jenis';
    return redirectTo(errorUrl('/jenis', message));
  }
};
