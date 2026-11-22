import type { ProjectInput, ProjectStatus } from './api';

const STATUSES: ProjectStatus[] = ['planning', 'active', 'on_hold', 'completed'];

/** Mengubah data formulir HTML menjadi payload untuk API project. */
export function readProjectForm(form: FormData): ProjectInput {
  const status = String(form.get('status') ?? 'planning') as ProjectStatus;
  // Jenis bisa lebih dari satu (checkbox dengan nama yang sama); divalidasi di server.
  const projectTypes = form
    .getAll('project_type')
    .map((v) => String(v).trim())
    .filter(Boolean);
  const optional = (key: string) => {
    const value = String(form.get(key) ?? '').trim();
    return value === '' ? null : value;
  };
  return {
    name: String(form.get('name') ?? ''),
    description: String(form.get('description') ?? ''),
    status: STATUSES.includes(status) ? status : 'planning',
    project_types: projectTypes.length ? projectTypes : ['lainnya'],
    owner_name: optional('owner_name'),
    start_date: optional('start_date'),
    due_date: optional('due_date'),
  };
}
