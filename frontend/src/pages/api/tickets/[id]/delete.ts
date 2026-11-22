import type { APIRoute } from 'astro';
import { api, ApiError } from '../../../../lib/api';
import { errorUrl, okUrl, redirectTo, safeId } from '../../../../lib/http';

export const POST: APIRoute = async ({ params, request, locals }) => {
  const id = safeId(params.id ?? null);
  const form = await request.formData();
  const projectId = safeId(String(form.get('project_id') ?? ''));
  if (!id || !projectId) return redirectTo('/dashboard');

  const back = `/projects/${projectId}#tiket`;
  try {
    await api(`/api/tickets/${id}`, { method: 'DELETE', token: locals.token });
    return redirectTo(okUrl(back, 'Tiket dihapus'));
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal menghapus tiket';
    return redirectTo(errorUrl(back, message));
  }
};
