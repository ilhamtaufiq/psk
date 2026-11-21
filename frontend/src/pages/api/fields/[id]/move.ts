import type { APIRoute } from 'astro';
import { api, ApiError } from '../../../../lib/api';
import { backTo, errorUrl, redirectTo, safeId } from '../../../../lib/http';

export const POST: APIRoute = async ({ params, request, locals }) => {
  const id = safeId(params.id ?? null);
  const form = await request.formData();
  const projectId = safeId(form.get('project_id'));
  if (!id || !projectId) return redirectTo('/dashboard');

  const direction = String(form.get('direction') ?? '');
  const back = backTo(projectId, '#informasi');
  try {
    await api(`/api/fields/${id}/move`, {
      method: 'POST',
      token: locals.token,
      json: { direction },
    });
    return redirectTo(back);
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal memindahkan informasi';
    return redirectTo(errorUrl(back, message));
  }
};
