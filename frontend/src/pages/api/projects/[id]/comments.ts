import type { APIRoute } from 'astro';
import { api, ApiError } from '../../../../lib/api';
import { backTo, errorUrl, redirectTo, safeId, okUrl } from '../../../../lib/http';

export const POST: APIRoute = async ({ params, request, locals }) => {
  const id = safeId(params.id ?? null);
  if (!id) return redirectTo('/dashboard');

  const form = await request.formData();
  const back = backTo(id, '#komentar');
  try {
    await api(`/api/projects/${id}/comments`, {
      method: 'POST',
      token: locals.token,
      json: { body: String(form.get('body') ?? '') },
    });
    return redirectTo(okUrl(back, 'Komentar terkirim'));
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal mengirim komentar';
    return redirectTo(errorUrl(back, message));
  }
};
