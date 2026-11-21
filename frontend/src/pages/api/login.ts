import type { APIRoute } from 'astro';
import { api, ApiError } from '../../lib/api';
import { errorUrl, redirectTo, setSessionCookie } from '../../lib/http';

export const POST: APIRoute = async ({ request, cookies }) => {
  const form = await request.formData();
  try {
    const data = await api<{ token: string }>('/api/auth/login', {
      method: 'POST',
      json: {
        email: String(form.get('email') ?? ''),
        password: String(form.get('password') ?? ''),
      },
    });
    setSessionCookie(cookies, data.token, request);
    return redirectTo('/dashboard');
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal masuk, coba lagi';
    return redirectTo(errorUrl('/login', message));
  }
};
