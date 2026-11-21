import type { ProjectInput, ProjectStatus, ProjectType } from './api';

const STATUSES: ProjectStatus[] = ['planning', 'active', 'on_hold', 'completed'];
const TYPES: ProjectType[] = ['administrasi', 'konstruksi', 'lainnya', 'operasional', 'pemasaran', 'riset', 'teknologi'];

/** Mengubah data formulir HTML menjadi payload untuk API proyek. */
export function readProjectForm(form: FormData): ProjectInput {
  const status = String(form.get('status') ?? 'planning') as ProjectStatus;
  const projectType = String(form.get('project_type') ?? 'lainnya') as ProjectType;
  const optional = (key: string) => {
    const value = String(form.get(key) ?? '').trim();
    return value === '' ? null : value;
  };
  return {
    name: String(form.get('name') ?? ''),
    description: String(form.get('description') ?? ''),
    status: STATUSES.includes(status) ? status : 'planning',
    project_type: TYPES.includes(projectType) ? projectType : 'lainnya',
    start_date: optional('start_date'),
    due_date: optional('due_date'),
  };
}
