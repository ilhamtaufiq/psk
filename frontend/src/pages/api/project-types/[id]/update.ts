import type { APIRoute } from 'astro';
import { api, ApiError } from '../../../../lib/api';
import { errorUrl, redirectTo, safeId, okUrl } from '../../../../lib/http';

export const POST: APIRoute = async ({ params, request, locals }) => {
  const id = safeId(params.id ?? null);
  if (!id) return redirectTo('/jenis');

  const form = await request.formData();
  try {
    await api(`/api/project-types/${id}`, {
      method: 'PUT',
      token: locals.token,
      json: { label: String(form.get('label') ?? '') },
    });
    return redirectTo(okUrl('/jenis', 'Nama jenis diubah'));
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal mengubah jenis';
    return redirectTo(errorUrl('/jenis', message));
  }
};
