import type { APIRoute } from 'astro';
import { api, ApiError } from '../../../../lib/api';
import { backTo, errorUrl, redirectTo, safeId, okUrl } from '../../../../lib/http';

export const POST: APIRoute = async ({ params, request, locals }) => {
  const id = safeId(params.id ?? null);
  if (!id) return redirectTo('/dashboard');

  const incoming = await request.formData();
  // Teruskan apa adanya: teks komentar dan setiap berkas lampiran.
  const outgoing = new FormData();
  outgoing.set('body', String(incoming.get('body') ?? ''));
  for (const value of incoming.getAll('attachment')) {
    if (value instanceof File && value.size > 0) outgoing.append('attachment', value, value.name);
  }

  const back = backTo(id, '#komentar');
  try {
    await api(`/api/projects/${id}/comments`, { method: 'POST', token: locals.token, body: outgoing });
    return redirectTo(okUrl(back, 'Komentar terkirim'));
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal mengirim komentar';
    return redirectTo(errorUrl(back, message));
  }
};
