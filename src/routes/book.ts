import { getBookInfo } from "../helpers/cover";
import { renderView } from "../helpers/renderers";
import mime from 'mime-types';
import * as fs from 'fs';
import * as path from 'path';
import { Hono } from "hono";
import { getConfig } from '../config';
import { DOMParser } from "linkedom";

const app = new Hono();
const config = getConfig();

// Only allow plain filenames inside BOOKS_DIR, rejecting anything like "../"
const resolveBookPath = (filename: string): string | null => {
  const booksDir = path.resolve(config.BOOKS_DIR);
  const filePath = path.resolve(booksDir, path.basename(filename));
  return path.dirname(filePath) === booksDir && !path.basename(filePath).startsWith('.') ? filePath : null;
};

app.get('/:filename', async (c) => {
  const filePath = resolveBookPath(c.req.param('filename'));

  if (filePath && fs.existsSync(filePath)) {
    const stat = fs.statSync(filePath);
    c.header('Content-Type', mime.lookup(filePath) || 'application/octet-stream');
    c.header('Content-Length', stat.size.toString());
    return c.body(fs.createReadStream(filePath) as any);
  }
  return c.notFound();
});

app.post('/upload', async (c) => {
  const body = await c.req.parseBody();
  const file = body['book'];

  if (file instanceof File) {
    if (!file.name) return c.text('No filename', 400);

    const dest = resolveBookPath(file.name);
    if (!dest) return c.text('Invalid filename', 400);
    console.log(`Uploading ${file.name}`);

    try {
      await fs.promises.writeFile(dest, Buffer.from(await file.arrayBuffer()));
    } catch (err) {
      console.error(err);
      return c.text('Upload failed', 500);
    }
  }

  return c.redirect('/admin');
});

app.post('/delete/:filename', async (c) => {
  const filePath = resolveBookPath(c.req.param('filename'));

  try {
    if (filePath && fs.existsSync(filePath)) await fs.promises.unlink(filePath);
  } catch (err) {
    console.error(err);
  }

  return c.redirect('/admin');
});

app.post('/rename', async (c) => {
  const { oldFilename, newFilename } = await c.req.parseBody();

  if (oldFilename && newFilename) {
    const safeOld = path.basename(oldFilename as string);
    let safeNew = path.basename(newFilename as string);

    if (!path.extname(safeNew)) safeNew += path.extname(safeOld);

    const oldPath = resolveBookPath(safeOld);
    const newPath = resolveBookPath(safeNew);

    try {
      if (oldPath && newPath && fs.existsSync(oldPath) && !fs.existsSync(newPath)) {
        await fs.promises.rename(oldPath, newPath);
      }
    } catch (err) {
      console.error(err);
    }
  }

  return c.redirect('/admin');
});

app.get('/info/:filename', async (c) => {
  const filename = c.req.param('filename');
  const filePath = resolveBookPath(filename);
  if (!filePath || !fs.existsSync(filePath)) return c.notFound();

  const bookInfo = await getBookInfo(filePath);
  if (bookInfo) {
    // EPUB descriptions often contain HTML: keep only the text so it is rendered escaped
    if (bookInfo.description) {
      bookInfo.description = new DOMParser().parseFromString(`<html><body>${bookInfo.description}</body></html>`, 'text/html').body.textContent?.trim();
    }
    const html = await renderView('book_details', {
      book: bookInfo,
      filename: filename,
      title: bookInfo.title || filename
    });
    return c.html(html);
  }

  return c.notFound();
});

app.get('/cover/:filename', async (c) => {
  const filePath = resolveBookPath(c.req.param('filename'));
  if (!filePath || !fs.existsSync(filePath)) return c.notFound();

  const bookInfo = await getBookInfo(filePath);
  if (bookInfo?.cover) {
    c.header('Cache-Control', 'public, max-age=86400');
    return c.body(bookInfo.cover as any);
  }

  return c.notFound();
});


export default app;
