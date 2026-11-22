import type { APIRoute } from 'astro';
import { api, ApiError, type ProjectComment } from '../../../../lib/api';
import { errorUrl, redirectTo, safeId } from '../../../../lib/http';

export const POST: APIRoute = async ({ params, request, locals }) => {
  const id = safeId(params.id ?? null);
  if (!id) return redirectTo('/dashboard');

  const incoming = await request.formData();
  const sort = String(incoming.get('sort') ?? '') === 'new' ? 'new' : 'top';
  const parentId = safeId(String(incoming.get('parent_id') ?? ''));

  // Teruskan teks komentar, id induk (untuk balasan), dan setiap berkas lampiran.
  const outgoing = new FormData();
  outgoing.set('body', String(incoming.get('body') ?? ''));
  if (parentId) outgoing.set('parent_id', parentId);
  for (const value of incoming.getAll('attachment')) {
    if (value instanceof File && value.size > 0) outgoing.append('attachment', value, value.name);
  }

  const back = `/projects/${id}${sort === 'new' ? '?sort=new' : ''}#komentar`;
  try {
    const created = await api<ProjectComment>(`/api/projects/${id}/comments`, {
      method: 'POST',
      token: locals.token,
      body: outgoing,
    });
    // Kembali ke komentar yang baru dibuat (atau induknya saat membalas).
    const anchor = `#c-${created.id}`;
    return redirectTo(`/projects/${id}${sort === 'new' ? '?sort=new' : ''}${anchor}`);
  } catch (err) {
    const message = err instanceof ApiError ? err.message : 'Gagal mengirim komentar';
    return redirectTo(errorUrl(back, message));
  }
};
