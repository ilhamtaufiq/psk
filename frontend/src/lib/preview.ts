export type PreviewKind = 'image' | 'pdf' | 'docx' | 'sheet' | 'pptx' | 'text' | 'none';

const KIND: Record<string, PreviewKind> = {
  png: 'image', jpg: 'image', jpeg: 'image', gif: 'image', webp: 'image',
  avif: 'image', bmp: 'image', svg: 'image',
  pdf: 'pdf',
  docx: 'docx',
  xlsx: 'sheet', xlsm: 'sheet', xls: 'none', csv: 'sheet', ods: 'sheet',
  pptx: 'pptx',
  txt: 'text', md: 'text', log: 'text', json: 'text',
  // Format Office lama (.doc, .ppt) tidak didukung pratinjaunya.
};

/** Jenis pratinjau dari nama berkas. */
export function previewKind(name: string): PreviewKind {
  const dot = name.lastIndexOf('.');
  const ext = dot > 0 ? name.slice(dot + 1).toLowerCase() : '';
  return KIND[ext] ?? 'none';
}
