import type { APIRoute } from 'astro';
import { api, ApiError, type Requirement } from '../../../../lib/api';
import { backTo, errorUrl, redirectTo, safeId } from '../../../../lib/http';

export const POST: APIRoute = async ({ params, request, locals }) => {
  const id = safeId(params.id ?? null);
  if (!id) return redirectTo('/dashboard');

  const form = await request.formData();
  const back = backTo(id, '#kelengkapan');
  try {
    await api<Requirement>(`/api/projects/${id}/requirements`, {
      method: 'POST',
      token: locals.token,
      json: { title: String(form.get('title') ?? '') },
    });
    return redirectTo(back);
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal menambahkan item';
    return redirectTo(errorUrl(back, message));
  }
};
