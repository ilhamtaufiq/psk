import type { APIRoute } from 'astro';
import { apiRaw, ApiError } from '../../../lib/api';
import { redirectTo, safeId } from '../../../lib/http';

// Meneruskan unduhan/pratinjau dari backend ke peramban tanpa memuat seluruh berkas ke memori.
export const GET: APIRoute = async ({ params, request, locals }) => {
  const id = safeId(params.id ?? null);
  if (!id) return new Response('Not found', { status: 404 });

  const inline = new URL(request.url).searchParams.get('inline');
  const query = inline === '1' ? '?inline=1' : '';

  try {
    const upstream = await apiRaw(`/api/files/${id}${query}`, { token: locals.token });
    const headers = new Headers();
    for (const name of [
      'content-type',
      'content-length',
      'content-disposition',
      'content-security-policy',
      'x-content-type-options',
      'cache-control',
    ]) {
      const value = upstream.headers.get(name);
      if (value) headers.set(name, value);
    }
    return new Response(upstream.body, { status: 200, headers });
  } catch (err) {
    if (err instanceof ApiError && err.status === 401) return redirectTo('/login');
    return new Response('Berkas tidak ditemukan', { status: 404 });
  }
};
