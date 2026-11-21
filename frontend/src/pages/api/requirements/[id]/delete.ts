import type { APIRoute } from 'astro';
import { api, ApiError } from '../../../../lib/api';
import { backTo, errorUrl, redirectTo, safeId, okUrl } from '../../../../lib/http';

export const POST: APIRoute = async ({ params, request, locals }) => {
  const id = safeId(params.id ?? null);
  const form = await request.formData();
  const projectId = safeId(form.get('project_id'));
  if (!id || !projectId) return redirectTo('/dashboard');

  const back = backTo(projectId, '#kelengkapan');
  try {
    await api(`/api/requirements/${id}`, { method: 'DELETE', token: locals.token });
    return redirectTo(okUrl(back, 'Item dihapus dari daftar'));
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal menghapus item';
    return redirectTo(errorUrl(back, message));
  }
};
