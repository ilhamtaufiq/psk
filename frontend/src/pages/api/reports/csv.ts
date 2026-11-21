import type { APIRoute } from 'astro';
import { apiRaw, ApiError } from '../../../lib/api';
import { redirectTo } from '../../../lib/http';

// Meneruskan unduhan CSV laporan dari backend dengan filter yang sama dengan halaman laporan.
export const GET: APIRoute = async ({ request, locals }) => {
  const incoming = new URL(request.url).searchParams;
  const query = new URLSearchParams();
  const status = incoming.get('status');
  const type = incoming.get('type');
  if (status) query.set('status', status);
  if (type) query.set('type', type);
  const qs = query.toString();

  try {
    const upstream = await apiRaw(`/api/reports/projects.csv${qs ? `?${qs}` : ''}`, { token: locals.token });
    const headers = new Headers();
    headers.set('content-type', 'text/csv; charset=utf-8');
    const disposition = upstream.headers.get('content-disposition');
    if (disposition) headers.set('content-disposition', disposition);
    headers.set('cache-control', 'private, no-store');
    return new Response(upstream.body, { status: 200, headers });
  } catch (err) {
    if (err instanceof ApiError && err.status === 401) return redirectTo('/login');
    return new Response('Laporan tidak dapat dibuat', { status: 500 });
  }
};
