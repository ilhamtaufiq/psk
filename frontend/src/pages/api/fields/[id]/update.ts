import type { APIRoute } from 'astro';
import { api, ApiError, type ProjectField } from '../../../../lib/api';
import { backTo, errorUrl, redirectTo, safeId } from '../../../../lib/http';

export const POST: APIRoute = async ({ params, request, locals }) => {
  const id = safeId(params.id ?? null);
  const form = await request.formData();
  const projectId = safeId(form.get('project_id'));
  if (!id || !projectId) return redirectTo('/dashboard');

  const back = backTo(projectId, '#informasi');
  try {
    await api<ProjectField>(`/api/fields/${id}`, {
      method: 'PUT',
      token: locals.token,
      json: {
        label: String(form.get('label') ?? ''),
        field_type: String(form.get('field_type') ?? 'text'),
        value: String(form.get('value') ?? ''),
      },
    });
    return redirectTo(back);
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal mengubah informasi';
    return redirectTo(errorUrl(back, message));
  }
};
