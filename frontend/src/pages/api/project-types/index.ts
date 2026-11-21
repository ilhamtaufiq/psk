import type { APIRoute } from 'astro';
import { api, ApiError } from '../../../lib/api';
import { errorUrl, redirectTo, okUrl } from '../../../lib/http';

export const POST: APIRoute = async ({ request, locals }) => {
  const form = await request.formData();
  try {
    await api('/api/project-types', {
      method: 'POST',
      token: locals.token,
      json: { label: String(form.get('label') ?? '') },
    });
    return redirectTo(okUrl('/jenis', 'Jenis ditambahkan'));
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal menambahkan jenis';
    return redirectTo(errorUrl('/jenis', message));
  }
};
