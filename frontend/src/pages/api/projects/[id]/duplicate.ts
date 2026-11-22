import type { APIRoute } from 'astro';
import { api, ApiError, type Project } from '../../../../lib/api';
import { errorUrl, redirectTo, safeId, okUrl } from '../../../../lib/http';

export const POST: APIRoute = async ({ params, locals }) => {
  const id = safeId(params.id ?? null);
  if (!id) return redirectTo('/dashboard');

  try {
    const copy = await api<Project>(`/api/projects/${id}/duplicate`, {
      method: 'POST',
      token: locals.token,
    });
    // Langsung ke salinan agar pengguna bisa segera mengubah namanya.
    return redirectTo(okUrl(`/projects/${copy.id}/edit`, 'Project diduplikat'));
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal menduplikat project';
    return redirectTo(errorUrl(`/projects/${id}`, message));
  }
};
