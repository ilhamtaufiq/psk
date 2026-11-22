import type { APIRoute } from 'astro';
import { api, ApiError } from '../../../../lib/api';
import { redirectTo, safeId } from '../../../../lib/http';

// Membuka notifikasi: ditandai sudah dibaca, lalu diarahkan ke tujuannya.
export const GET: APIRoute = async ({ params, locals }) => {
  const id = safeId(params.id ?? null);
  if (!id) return redirectTo('/dashboard');
  try {
    const opened = await api<{ link: string }>(`/api/notifications/${id}/read`, {
      method: 'PUT',
      token: locals.token,
    });
    // Hanya tujuan internal (diawali "/") yang diizinkan.
    const target = opened.link.startsWith('/') && !opened.link.startsWith('//') ? opened.link : '/dashboard';
    return redirectTo(target);
  } catch (err) {
    if (err instanceof ApiError && err.status === 401) return redirectTo('/login');
    return redirectTo('/dashboard');
  }
};
