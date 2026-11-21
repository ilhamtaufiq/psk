import type { APIRoute } from 'astro';
import { api, ApiError, type Requirement } from '../../../../lib/api';
import { backTo, errorUrl, redirectTo, safeId, okUrl } from '../../../../lib/http';

export const POST: APIRoute = async ({ params, request, locals }) => {
  const id = safeId(params.id ?? null);
  const form = await request.formData();
  const projectId = safeId(form.get('project_id'));
  if (!id || !projectId) return redirectTo('/dashboard');

  const back = backTo(projectId, '#kelengkapan');
  try {
    await api<Requirement>(`/api/requirements/${id}`, {
      method: 'PUT',
      token: locals.token,
      json: { title: String(form.get('title') ?? '') },
    });
    return redirectTo(okUrl(back, 'Nama item diubah'));
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal mengubah item';
    return redirectTo(errorUrl(back, message));
  }
};
