// Pembaruan langsung halaman detail project: saat ada perubahan dari backend (berkas, kelengkapan,
// informasi, anggota, atau project), bagian .detail diambil ulang tanpa reload penuh.
// Dialog pratinjau dan form yang sedang diisi tidak diganggu.

const root = document.querySelector<HTMLElement>('[data-live-project]');

if (root) {
  const projectId = root.dataset.liveProject ?? '';
  let timer: number | undefined;
  let pending = false;

  // Jangan ganti bagian halaman saat pengguna sedang mengetik; tunggu sampai fokus pindah.
  const isEditing = () => {
    const el = document.activeElement;
    if (!(el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement)) {
      return false;
    }
    return !!el.closest('.detail');
  };

  const refresh = async () => {
    const current = document.querySelector<HTMLElement>('[data-live-project]');
    if (!current) return;
    if (isEditing()) {
      pending = true;
      return;
    }
    pending = false;

    try {
      const res = await fetch(location.pathname, {
        credentials: 'same-origin',
        cache: 'no-store',
        headers: { accept: 'text/html' },
      });
      // Sesi habis, project dihapus, atau akses dicabut: biarkan halaman apa adanya.
      if (!res.ok || res.redirected) return;

      const html = await res.text();
      const doc = new DOMParser().parseFromString(html, 'text/html');
      const fresh = doc.querySelector<HTMLElement>('[data-live-project]');
      if (!fresh || fresh.dataset.liveProject !== projectId) return;

      // Pertahankan panel yang sedang terbuka (mis. menu Ubah) berdasarkan urutannya.
      const openStates = Array.from(current.querySelectorAll('details')).map((d) => d.open);
      current.replaceWith(fresh);
      fresh.querySelectorAll('details').forEach((d, i) => {
        if (openStates[i]) d.open = true;
      });
    } catch {
      // Jaringan putus: pembaruan berikutnya akan mencoba lagi.
    }
  };

  const schedule = () => {
    window.clearTimeout(timer);
    timer = window.setTimeout(refresh, 250);
  };

  const source = new EventSource(`/api/projects/${projectId}/events`);
  let wasDisconnected = false;

  source.addEventListener('change', schedule);
  source.onerror = () => {
    wasDisconnected = true;
  };
  source.onopen = () => {
    // Setelah putus lalu tersambung lagi, ambil ulang untuk menutup perubahan yang terlewat.
    if (wasDisconnected) {
      wasDisconnected = false;
      schedule();
    }
  };

  document.addEventListener('focusout', () => {
    if (pending) window.setTimeout(refresh, 0);
  });
}
