export interface Book {
  title: string;
  filename: string;
  size: number;
  mimeType: string;
  lastUpdated: string;
  // Helpers for sorting/filtering
  simpleMime?: string;
}

export type SortMode = 'name-asc' | 'name-desc' | 'date-asc' | 'date-desc';

export interface TemplateData {
  books: Book[];
  baseUrl: string;
  currentTime: string;
  sortMode: SortMode;
}

export interface BookInfo {
  [x: string]: any;
  title?: string;
  creator?: string;
  identifier?: string;
  language?: string;
  publisher?: string;
  subject?: string;
  description?: string;
  date?: string;
  cover?: Buffer | Uint8Array;
}
