import type { APIRoute } from 'astro';
import { api, ApiError, type ProjectFile } from '../../../../lib/api';
import { backTo, errorUrl, redirectTo, safeId, okUrl } from '../../../../lib/http';

const MAX_BYTES = 25 * 1024 * 1024;

export const POST: APIRoute = async ({ params, request, locals }) => {
  const id = safeId(params.id ?? null);
  if (!id) return redirectTo('/dashboard');

  const form = await request.formData();
  const back = backTo(id, form.get('anchor'));
  const file = form.get('file');
  if (!(file instanceof File) || file.size === 0) {
    return redirectTo(errorUrl(back, 'Pilih berkas terlebih dahulu'));
  }
  if (file.size > MAX_BYTES) {
    return redirectTo(errorUrl(back, 'Ukuran berkas maksimal 25 MB'));
  }

  const body = new FormData();
  body.append('file', file, file.name);
  // Berkas bisa dikaitkan ke satu item kelengkapan; id-nya divalidasi di backend.
  const requirementId = safeId(form.get('requirement_id'));
  if (requirementId) body.append('requirement_id', requirementId);

  try {
    await api<ProjectFile>(`/api/projects/${id}/files`, {
      method: 'POST',
      token: locals.token,
      body,
    });
    return redirectTo(okUrl(back, 'Berkas diunggah'));
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal mengunggah berkas';
    return redirectTo(errorUrl(back, message));
  }
};
