import type { APIRoute } from 'astro';
import { api, ApiError } from '../../../../lib/api';
import { errorUrl, okUrl, redirectTo, safeId } from '../../../../lib/http';

// Formulir ubah tiket selalu mengirim status, prioritas, dan penanggung jawab sekaligus.
export const POST: APIRoute = async ({ params, request, locals }) => {
  const id = safeId(params.id ?? null);
  const form = await request.formData();
  const projectId = safeId(String(form.get('project_id') ?? ''));
  if (!id || !projectId) return redirectTo('/dashboard');

  const filter = String(form.get('filter') ?? '');
  const query = ['open', 'in_progress', 'done'].includes(filter) ? `?status=${filter}` : '';
  const back = `/projects/${projectId}${query}#t-${id}`;
  const assignee = String(form.get('assignee_id') ?? '').trim();
  try {
    await api(`/api/tickets/${id}`, {
      method: 'PUT',
      token: locals.token,
      json: {
        status: String(form.get('status') ?? ''),
        priority: String(form.get('priority') ?? ''),
        assignee_id: assignee === '' ? null : safeId(assignee),
      },
    });
    return redirectTo(okUrl(back, 'Tiket diperbarui'));
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal memperbarui tiket';
    return redirectTo(errorUrl(back, message));
  }
};
