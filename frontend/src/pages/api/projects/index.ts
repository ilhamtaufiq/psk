import type { APIRoute } from 'astro';
import { api, ApiError, type Project } from '../../../lib/api';
import { errorUrl, redirectTo, okUrl } from '../../../lib/http';
import { readProjectForm } from '../../../lib/projects-form';

export const POST: APIRoute = async ({ request, locals }) => {
  const form = await request.formData();
  try {
    const project = await api<Project>('/api/projects', {
      method: 'POST',
      token: locals.token,
      json: readProjectForm(form),
    });
    return redirectTo(okUrl(`/projects/${project.id}`, 'Proyek dibuat'));
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal membuat proyek';
    return redirectTo(errorUrl('/projects/new', message));
  }
};
