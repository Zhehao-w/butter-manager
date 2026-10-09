export type SaveValue = string | number | boolean | null;
export type SaveField = {
  id: string;
  name: string;
  path: string;
  category: string;
  kind: 'number' | 'string' | 'boolean' | 'readonly';
  value: SaveValue;
  editable: boolean;
  reason: string | null;
  description: string | null;
};
export type SaveSlot = { id: string; name: string; format: string; modified: number };
export type SaveCatalog = { slots: SaveSlot[]; warnings: string[] };
export type SaveDocument = {
  slot: SaveSlot;
  revision: string;
  fields: SaveField[];
  metadata: string[];
  screenshot: string | null;
  warnings: string[];
};
export type SaveChange = { id: string; value: SaveValue };
