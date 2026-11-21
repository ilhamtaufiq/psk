import type { APIRoute } from 'astro';
import { api } from '../../lib/api';
import { clearSessionCookie, redirectTo } from '../../lib/http';

export const POST: APIRoute = async ({ locals, cookies }) => {
  if (locals.token) {
    try {
      await api('/api/auth/logout', { method: 'POST', token: locals.token });
    } catch {
      /* sesi di backend mungkin sudah tidak ada; cookie tetap dihapus */
    }
  }
  clearSessionCookie(cookies);
  return redirectTo('/login');
};
