import type { APIRoute } from 'astro';
import { api, ApiError } from '../../../../lib/api';
import { backTo, errorUrl, redirectTo, safeId, okUrl } from '../../../../lib/http';

export const POST: APIRoute = async ({ params, request, locals }) => {
  const id = safeId(params.id ?? null);
  const form = await request.formData();
  // Id proyek dikirim lewat formulir agar peramban kembali ke halaman proyek yang sama.
  const projectId = safeId(form.get('project_id'));
  if (!id || !projectId) return redirectTo('/dashboard');
  const back = backTo(projectId, form.get('anchor'));

  try {
    await api(`/api/files/${id}`, { method: 'DELETE', token: locals.token });
    return redirectTo(okUrl(back, 'Berkas dihapus'));
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal menghapus berkas';
    return redirectTo(errorUrl(back, message));
  }
};
