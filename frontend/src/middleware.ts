import { defineMiddleware } from 'astro:middleware';
import { api, ApiError, type User } from './lib/api';
import { SESSION_COOKIE, clearSessionCookie } from './lib/http';

export const onRequest = defineMiddleware(async (context, next) => {
  const { pathname } = context.url;
  let token = context.cookies.get(SESSION_COOKIE)?.value ?? null;
  context.locals.token = token;
  context.locals.user = undefined;

  // Validasi sesi hanya untuk halaman; endpoint /api/* memvalidasi sendiri saat memanggil backend.
  const isPage = !pathname.startsWith('/api/');
  if (isPage && token) {
    try {
      context.locals.user = await api<User>('/api/auth/me', { token });
    } catch (err) {
      if (err instanceof ApiError && err.status === 401) {
        clearSessionCookie(context.cookies);
        token = null;
        context.locals.token = null;
      } else {
        throw err;
      }
    }
  }

  const isProtected =
    pathname === '/' || pathname.startsWith('/projects') || pathname === '/jenis' || pathname === '/laporan' || pathname === '/profile';
  const isGuestOnly = pathname === '/login' || pathname === '/register';

  if (isProtected && !context.locals.user) {
    return context.redirect('/login');
  }
  if (isGuestOnly && context.locals.user) {
    return context.redirect('/dashboard');
  }
  return next();
});
