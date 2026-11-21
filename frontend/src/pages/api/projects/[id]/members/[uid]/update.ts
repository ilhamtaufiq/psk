import type { APIRoute } from 'astro';
import { api, ApiError, type Member } from '../../../../../../lib/api';
import { backTo, errorUrl, redirectTo, safeId } from '../../../../../../lib/http';

export const POST: APIRoute = async ({ params, request, locals }) => {
  const id = safeId(params.id ?? null);
  const uid = safeId(params.uid ?? null);
  if (!id || !uid) return redirectTo('/dashboard');

  const form = await request.formData();
  const back = backTo(id, '#anggota');
  try {
    await api<Member>(`/api/projects/${id}/members/${uid}`, {
      method: 'PUT',
      token: locals.token,
      json: { role: String(form.get('role') ?? '') },
    });
    return redirectTo(back);
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal mengubah peran';
    return redirectTo(errorUrl(back, message));
  }
};
