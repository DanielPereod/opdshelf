import fs from 'fs';
import path from 'path';
import mime from 'mime-types';
import { Book, SortMode } from './types';
import { getConfig } from './config';

const MIME_MAP: Record<string, string> = {
  'application/epub+zip': 'EPUB',
  'application/pdf': 'PDF',
  'application/x-fictionbook+xml': 'FB2',
  'application/x-zip-compressed-fb2': 'FB2',
  'application/zip': 'ZIP',
  'application/x-zip-compressed': 'ZIP',
  'application/x-cbz': 'CBZ',
  'application/vnd.comicbook+zip': 'CBZ',
  'application/x-cbr': 'CBR',
  'application/x-mobi': 'MOBI',
  'application/x-mobipocket-ebook': 'MOBI',
  'application/vnd.amazon.ebook': 'AZW',
  'image/vnd.djvu': 'DJVU',
  'text/plain': 'TXT',
  'text/rtf': 'RTF',
  'application/rtf': 'RTF',
  'text/html': 'HTML',
};

export const getSimpleMime = (mimeType: string): string => {
  if (MIME_MAP[mimeType]) return MIME_MAP[mimeType];

  const lower = mimeType.toLowerCase();
  if (lower.includes('azw')) return 'AZW';
  if (lower.includes('djvu')) return 'DJVU';

  return mimeType.length > 12 ? mimeType.substring(0, 10) + '...' : mimeType;
};

export const formatSize = (bytes: number): string => {
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let size = bytes;
  let i = 0;

  while (size >= 1024 && i < units.length - 1) {
    size /= 1024;
    i++;
  }

  return `${size.toFixed(1)} ${units[i]}`;
};

export const getBooks = async (dir: string): Promise<Book[]> => {
  const config = getConfig();
  
  try {
    if (!fs.existsSync(dir)) {
      await fs.promises.mkdir(dir, { recursive: true });
      return [];
    }

    const files = await fs.promises.readdir(dir);
    const books: Book[] = [];

    for (const file of files) {
      if (file.startsWith('.')) continue;

      const filePath = path.join(dir, file);
      const stats = await fs.promises.stat(filePath);

      if (stats.isFile()) {
        const mimeType = mime.lookup(filePath) || 'application/octet-stream';
        books.push({
          title: path.basename(file, path.extname(file)),
          filename: path.relative(config.BOOKS_DIR, filePath).replace(/\\/g, '/'),
          size: stats.size,
          mimeType,
          lastUpdated: stats.mtime.toISOString(),
          simpleMime: getSimpleMime(mimeType)
        });
      } else if (stats.isDirectory()) {
        const subBooks = await getBooks(filePath);
        books.push(...subBooks);
      }
    }

    return books;
  } catch (err) {
    console.error(err);
    return [];
  }
};

export const sortBooks = (books: Book[], mode: SortMode): Book[] => {
  return [...books].sort((a, b) => {
    switch (mode) {
      case 'name-asc': return a.title.localeCompare(b.title);
      case 'name-desc': return b.title.localeCompare(a.title);
      case 'date-asc': return a.lastUpdated.localeCompare(b.lastUpdated);
      case 'date-desc':
      default: return b.lastUpdated.localeCompare(a.lastUpdated);
    }
  });
};
