import type { APIRoute } from 'astro';
import { api, ApiError } from '../../../lib/api';
import { errorUrl, redirectTo, okUrl } from '../../../lib/http';

export const POST: APIRoute = async ({ request, locals }) => {
  const form = await request.formData();
  const newPassword = String(form.get('new_password') ?? '');
  const confirm = String(form.get('confirm_password') ?? '');
  if (newPassword !== confirm) {
    return redirectTo(errorUrl('/profile#password', 'Konfirmasi password tidak sama'));
  }
  try {
    await api('/api/profile/password', {
      method: 'PUT',
      token: locals.token,
      json: {
        current_password: String(form.get('current_password') ?? ''),
        new_password: newPassword,
      },
    });
    return redirectTo(okUrl('/profile#password', 'Password diganti. Sesi di perangkat lain sudah keluar.'));
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal mengganti password';
    return redirectTo(errorUrl('/profile#password', message));
  }
};
