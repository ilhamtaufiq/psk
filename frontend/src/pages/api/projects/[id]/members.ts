import type { APIRoute } from 'astro';
import { api, ApiError, type Member } from '../../../../lib/api';
import { backTo, errorUrl, redirectTo, safeId, okUrl } from '../../../../lib/http';

export const POST: APIRoute = async ({ params, request, locals }) => {
  const id = safeId(params.id ?? null);
  if (!id) return redirectTo('/dashboard');

  const form = await request.formData();
  const back = backTo(id, '#anggota');
  try {
    await api<Member>(`/api/projects/${id}/members`, {
      method: 'POST',
      token: locals.token,
      json: {
        email: String(form.get('email') ?? ''),
        role: String(form.get('role') ?? 'viewer'),
      },
    });
    return redirectTo(okUrl(back, 'Anggota diundang'));
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal mengundang anggota';
    return redirectTo(errorUrl(back, message));
  }
};
