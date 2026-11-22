import type { APIRoute } from 'astro';
import { api, ApiError } from '../../../../lib/api';
import { errorUrl, redirectTo, safeId } from '../../../../lib/http';

export const POST: APIRoute = async ({ params, request, locals }) => {
  const id = safeId(params.id ?? null);
  const form = await request.formData();
  const projectId = safeId(String(form.get('project_id') ?? ''));
  if (!id || !projectId) return redirectTo('/dashboard');

  const sort = String(form.get('sort') ?? '') === 'new' ? '?sort=new' : '';
  const back = `/projects/${projectId}${sort}#c-${id}`;
  const raw = Number(form.get('value') ?? 0);
  const value = raw === 1 || raw === -1 ? raw : 0;

  try {
    await api(`/api/comments/${id}/vote`, { method: 'POST', token: locals.token, json: { value } });
    return redirectTo(back);
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal memberi suara';
    return redirectTo(errorUrl(back, message));
  }
};
