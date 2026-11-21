import type { APIRoute } from 'astro';
import { API_URL } from '../../lib/api';

// Frontend dianggap hidup selama endpoint ini menjawab (dipakai Docker HEALTHCHECK).
// Status backend hanya dilaporkan, tidak membuat frontend dianggap mati.
export const GET: APIRoute = async () => {
  let backend: 'ok' | 'down' = 'down';
  try {
    const res = await fetch(`${API_URL}/api/health`, { signal: AbortSignal.timeout(2000) });
    backend = res.ok ? 'ok' : 'down';
  } catch {
    backend = 'down';
  }

  return new Response(
    JSON.stringify({
      status: backend === 'ok' ? 'ok' : 'degraded',
      backend,
      time: new Date().toISOString(),
    }),
    {
      status: 200,
      headers: { 'content-type': 'application/json', 'cache-control': 'no-store' },
    },
  );
};
