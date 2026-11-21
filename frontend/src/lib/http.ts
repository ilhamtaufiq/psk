import type { AstroCookies } from 'astro';

export const SESSION_COOKIE = 'psk_session';
const THIRTY_DAYS = 60 * 60 * 24 * 30;

function isSecureRequest(request: Request): boolean {
  return (
    new URL(request.url).protocol === 'https:' ||
    request.headers.get('x-forwarded-proto') === 'https'
  );
}

export function setSessionCookie(cookies: AstroCookies, token: string, request: Request) {
  cookies.set(SESSION_COOKIE, token, {
    httpOnly: true,
    sameSite: 'lax',
    secure: isSecureRequest(request),
    path: '/',
    maxAge: THIRTY_DAYS,
  });
}

export function clearSessionCookie(cookies: AstroCookies) {
  cookies.delete(SESSION_COOKIE, { path: '/' });
}

/** Redirect 303 setelah POST (pola PRG). */
export function redirectTo(location: string): Response {
  return new Response(null, { status: 303, headers: { Location: location } });
}

/** Membentuk URL dengan pesan error di query string. */
export function errorUrl(path: string, message: string): string {
  return `${path}?error=${encodeURIComponent(message)}`;
}

/**
 * Kembali ke halaman proyek dan ke bagian yang sama (agar posisi scroll tidak lompat).
 * Hanya dua anchor yang diizinkan.
 */
export function backTo(projectId: string, anchor: FormDataEntryValue | null): string {
  const safe = ['#informasi', '#kelengkapan', '#berkas'].includes(String(anchor)) ? String(anchor) : '';
  return `/projects/${projectId}${safe}`;
}

/** Hanya izinkan id berbentuk UUID di dalam URL redirect (mencegah open redirect). */
export function safeId(value: FormDataEntryValue | string | null): string | null {
  if (typeof value !== 'string') return null;
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value) ? value : null;
}
