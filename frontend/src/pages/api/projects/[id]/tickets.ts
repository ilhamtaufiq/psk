import type { APIRoute } from 'astro';
import { api, ApiError } from '../../../../lib/api';
import { errorUrl, okUrl, redirectTo, safeId } from '../../../../lib/http';

export const POST: APIRoute = async ({ params, request, locals }) => {
  const id = safeId(params.id ?? null);
  if (!id) return redirectTo('/dashboard');

  const incoming = await request.formData();
  // Teruskan teks tiket dan setiap berkas lampiran apa adanya.
  const outgoing = new FormData();
  outgoing.set('title', String(incoming.get('title') ?? ''));
  outgoing.set('description', String(incoming.get('description') ?? ''));
  outgoing.set('priority', String(incoming.get('priority') ?? 'normal'));
  for (const value of incoming.getAll('attachment')) {
    if (value instanceof File && value.size > 0) outgoing.append('attachment', value, value.name);
  }

  const back = `/projects/${id}#tiket`;
  try {
    await api(`/api/projects/${id}/tickets`, { method: 'POST', token: locals.token, body: outgoing });
    return redirectTo(okUrl(back, 'Tiket dibuat'));
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal membuat tiket';
    return redirectTo(errorUrl(back, message));
  }
};
