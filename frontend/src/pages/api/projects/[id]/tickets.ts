import type { APIRoute } from 'astro';
import { api, ApiError } from '../../../../lib/api';
import { errorUrl, okUrl, redirectTo, safeId } from '../../../../lib/http';

export const POST: APIRoute = async ({ params, request, locals }) => {
  const id = safeId(params.id ?? null);
  if (!id) return redirectTo('/dashboard');

  const form = await request.formData();
  const back = `/projects/${id}#tiket`;
  try {
    await api(`/api/projects/${id}/tickets`, {
      method: 'POST',
      token: locals.token,
      json: {
        title: String(form.get('title') ?? ''),
        description: String(form.get('description') ?? ''),
        priority: String(form.get('priority') ?? 'normal'),
      },
    });
    return redirectTo(okUrl(back, 'Tiket dibuat'));
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal membuat tiket';
    return redirectTo(errorUrl(back, message));
  }
};
