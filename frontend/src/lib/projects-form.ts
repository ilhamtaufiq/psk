import type { ProjectInput, ProjectStatus } from './api';

const STATUSES: ProjectStatus[] = ['planning', 'active', 'on_hold', 'completed'];

/** Mengubah data formulir HTML menjadi payload untuk API project. */
export function readProjectForm(form: FormData): ProjectInput {
  const status = String(form.get('status') ?? 'planning') as ProjectStatus;
  // Jenis divalidasi di server terhadap daftar milik pengguna.
  const projectType = String(form.get('project_type') ?? '').trim();
  const optional = (key: string) => {
    const value = String(form.get(key) ?? '').trim();
    return value === '' ? null : value;
  };
  return {
    name: String(form.get('name') ?? ''),
    description: String(form.get('description') ?? ''),
    status: STATUSES.includes(status) ? status : 'planning',
    project_type: projectType || 'lainnya',
    start_date: optional('start_date'),
    due_date: optional('due_date'),
  };
}
