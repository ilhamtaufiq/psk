import type { APIRoute } from 'astro';
import { api, ApiError } from '../../lib/api';
import { errorUrl, redirectTo, setSessionCookie } from '../../lib/http';

export const POST: APIRoute = async ({ request, cookies }) => {
  const form = await request.formData();
  try {
    const data = await api<{ token: string }>('/api/auth/register', {
      method: 'POST',
      json: {
        name: String(form.get('name') ?? ''),
        email: String(form.get('email') ?? ''),
        password: String(form.get('password') ?? ''),
      },
    });
    setSessionCookie(cookies, data.token, request);
    return redirectTo('/dashboard');
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal mendaftar, coba lagi';
    return redirectTo(errorUrl('/register', message));
  }
};
