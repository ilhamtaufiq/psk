import type { APIRoute } from 'astro';
import { api, ApiError } from '../../../../../../lib/api';
import { backTo, errorUrl, redirectTo, safeId, okUrl } from '../../../../../../lib/http';

export const POST: APIRoute = async ({ params, request, locals }) => {
  const id = safeId(params.id ?? null);
  const uid = safeId(params.uid ?? null);
  if (!id || !uid) return redirectTo('/dashboard');

  const form = await request.formData();
  // Anggota yang keluar sendiri tidak punya akses lagi, jadi kembali ke daftar proyek.
  const leaving = form.get('self') === '1';
  const back = leaving ? '/projects' : backTo(id, '#anggota');
  try {
    await api(`/api/projects/${id}/members/${uid}`, { method: 'DELETE', token: locals.token });
    return redirectTo(okUrl(back, leaving ? 'Anda keluar dari proyek' : 'Anggota dikeluarkan'));
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal mengeluarkan anggota';
    return redirectTo(errorUrl(leaving ? `/projects/${id}` : back, message));
  }
};
