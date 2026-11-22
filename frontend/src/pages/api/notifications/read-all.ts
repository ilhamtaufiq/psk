import type { APIRoute } from 'astro';
import { api, ApiError } from '../../../lib/api';
import { redirectTo } from '../../../lib/http';

// Menandai semua notifikasi sudah dibaca, lalu kembali ke halaman asal (hanya jika masih di situs ini).
export const POST: APIRoute = async ({ request, locals }) => {
  try {
    await api('/api/notifications/read-all', { method: 'POST', token: locals.token });
  } catch (err) {
    if (err instanceof ApiError && err.status === 401) return redirectTo('/login');
  }
  let back = '/dashboard';
  const referer = request.headers.get('referer');
  if (referer) {
    try {
      const url = new URL(referer);
      if (url.origin === new URL(request.url).origin) back = url.pathname + url.search + url.hash;
    } catch {
      /* referer tidak valid: pakai beranda */
    }
  }
  return redirectTo(back);
};
