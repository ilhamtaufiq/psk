import type { APIRoute } from 'astro';
import { apiRaw, ApiError } from '../../../lib/api';
import { redirectTo, safeId } from '../../../lib/http';

// Meneruskan unduhan lampiran tiket dari backend.
export const GET: APIRoute = async ({ params, locals }) => {
  const id = safeId(params.id ?? null);
  if (!id) return new Response('Not found', { status: 404 });

  try {
    const upstream = await apiRaw(`/api/ticket-attachments/${id}`, { token: locals.token });
    const headers = new Headers();
    for (const name of ['content-type', 'content-length', 'content-disposition', 'content-security-policy', 'x-content-type-options']) {
      const value = upstream.headers.get(name);
      if (value) headers.set(name, value);
    }
    return new Response(upstream.body, { status: 200, headers });
  } catch (err) {
    if (err instanceof ApiError && err.status === 401) return redirectTo('/login');
    return new Response('Lampiran tidak ditemukan', { status: 404 });
  }
};
