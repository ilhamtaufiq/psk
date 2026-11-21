import type { APIRoute } from 'astro';
import { api, ApiError, type ProjectFile } from '../../../../../lib/api';
import { backTo, errorUrl, redirectTo, safeId, okUrl } from '../../../../../lib/http';

export const POST: APIRoute = async ({ params, request, locals }) => {
  const id = safeId(params.id ?? null);
  if (!id) return redirectTo('/dashboard');

  const form = await request.formData();
  // Kembali ke bagian asal formulir: #berkas atau #kelengkapan.
  const back = backTo(id, form.get('anchor') ?? '#berkas');
  const url = String(form.get('url') ?? '').trim();
  const requirementId = safeId(form.get('requirement_id'));
  try {
    await api<ProjectFile>(`/api/projects/${id}/files/from-url`, {
      method: 'POST',
      token: locals.token,
      json: { url, requirement_id: requirementId },
    });
    return redirectTo(okUrl(back, 'Berkas diambil dari Google Drive'));
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal mengambil berkas dari Google Drive';
    return redirectTo(errorUrl(back, message));
  }
};
