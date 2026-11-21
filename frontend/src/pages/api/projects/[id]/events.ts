import type { APIRoute } from 'astro';
import { apiRaw, ApiError } from '../../../../lib/api';
import { safeId } from '../../../../lib/http';

// Meneruskan aliran Server-Sent Events dari backend. Token dibawa dari cookie di sisi server,
// karena EventSource di browser tidak bisa mengirim header Authorization.
export const GET: APIRoute = async ({ params, locals }) => {
  const id = safeId(params.id ?? null);
  if (!id) return new Response('Not found', { status: 404 });

  try {
    const upstream = await apiRaw(`/api/projects/${id}/events`, { token: locals.token });
    return new Response(upstream.body, {
      status: 200,
      headers: {
        'content-type': 'text/event-stream; charset=utf-8',
        'cache-control': 'no-cache, no-transform',
        'x-accel-buffering': 'no',
      },
    });
  } catch (err) {
    if (err instanceof ApiError && err.status === 401) return new Response('Unauthorized', { status: 401 });
    return new Response('Not found', { status: 404 });
  }
};
