import type { APIRoute } from 'astro';
import { api, ApiError, type Project } from '../../../../lib/api';
import { errorUrl, redirectTo, safeId } from '../../../../lib/http';

export const POST: APIRoute = async ({ params, locals }) => {
  const id = safeId(params.id ?? null);
  if (!id) return redirectTo('/dashboard');

  try {
    const copy = await api<Project>(`/api/projects/${id}/duplicate`, {
      method: 'POST',
      token: locals.token,
    });
    // Langsung ke salinan agar pengguna bisa segera mengubah namanya.
    return redirectTo(`/projects/${copy.id}/edit`);
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal menduplikat proyek';
    return redirectTo(errorUrl(`/projects/${id}`, message));
  }
};
