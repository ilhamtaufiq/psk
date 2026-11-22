import type { APIRoute } from 'astro';
import { api, ApiError, type User } from '../../../lib/api';
import { errorUrl, redirectTo, okUrl } from '../../../lib/http';

export const POST: APIRoute = async ({ request, locals }) => {
  const form = await request.formData();
  try {
    await api<User>('/api/profile', {
      method: 'PUT',
      token: locals.token,
      json: {
        name: String(form.get('name') ?? ''),
        email: String(form.get('email') ?? ''),
      },
    });
    return redirectTo(okUrl('/profile', 'Profil diperbarui'));
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal memperbarui profil';
    return redirectTo(errorUrl('/profile', message));
  }
};
