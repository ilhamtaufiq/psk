import type { APIRoute } from 'astro';
import { api, ApiError, type Project } from '../../../../lib/api';
import { errorUrl, redirectTo, safeId, okUrl } from '../../../../lib/http';
import { readProjectForm } from '../../../../lib/projects-form';

export const POST: APIRoute = async ({ params, request, locals }) => {
  const id = safeId(params.id ?? null);
  if (!id) return redirectTo('/dashboard');

  const form = await request.formData();
  try {
    await api<Project>(`/api/projects/${id}`, {
      method: 'PUT',
      token: locals.token,
      json: readProjectForm(form),
    });
    return redirectTo(okUrl(`/projects/${id}`, 'Perubahan disimpan'));
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal menyimpan perubahan';
    return redirectTo(errorUrl(`/projects/${id}/edit`, message));
  }
};
