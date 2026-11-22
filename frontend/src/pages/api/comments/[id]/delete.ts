import type { APIRoute } from 'astro';
import { api, ApiError } from '../../../../lib/api';
import { backTo, errorUrl, redirectTo, safeId, okUrl } from '../../../../lib/http';

export const POST: APIRoute = async ({ params, request, locals }) => {
  const commentId = safeId(params.id ?? null);
  const form = await request.formData();
  const projectId = safeId(String(form.get('project_id') ?? ''));
  if (!commentId || !projectId) return redirectTo('/dashboard');

  const back = backTo(projectId, '#komentar');
  try {
    await api(`/api/comments/${commentId}`, { method: 'DELETE', token: locals.token });
    return redirectTo(okUrl(back, 'Komentar dihapus'));
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal menghapus komentar';
    return redirectTo(errorUrl(back, message));
  }
};
