import type { ProjectComment } from './api';

export type ThreadNode = ProjectComment & { children: ThreadNode[] };

export type CommentSort = 'top' | 'new';

/** Menyusun komentar datar menjadi pohon balasan, lalu mengurutkan tiap tingkat. */
export function buildTree(list: ProjectComment[], sort: CommentSort): ThreadNode[] {
  const byId = new Map<string, ThreadNode>();
  for (const c of list) byId.set(c.id, { ...c, children: [] });

  const roots: ThreadNode[] = [];
  for (const node of byId.values()) {
    const parent = node.parent_id ? byId.get(node.parent_id) : undefined;
    if (parent) parent.children.push(node);
    else roots.push(node);
  }

  const byNewest = (a: ThreadNode, b: ThreadNode) => b.created_at.localeCompare(a.created_at);
  const compare =
    sort === 'top' ? (a: ThreadNode, b: ThreadNode) => b.score - a.score || byNewest(a, b) : byNewest;
  const sortLevel = (nodes: ThreadNode[]) => {
    nodes.sort(compare);
    nodes.forEach((n) => sortLevel(n.children));
  };
  sortLevel(roots);
  return roots;
}
