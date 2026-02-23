import { getBookInfo } from "../helpers/cover";
import { renderView } from "../helpers/renderers";
import mime from 'mime-types';
import * as fs from 'fs';
import * as path from 'path';
import { Hono } from "hono";
import { getConfig } from '../config';
import { BookInfo } from "../types";

const app = new Hono();
const config = getConfig();

app.post('/upload', async (c) => {
  const body = await c.req.parseBody();
  const file = body['book'];

  if (file instanceof File) {
    if (!file.name) return c.text('No filename', 400);

    const dest = path.join(config.BOOKS_DIR, file.name);
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

app.post('/delete/*', async (c) => {
  const fullPath = c.req.path;
  const deleteIndex = fullPath.indexOf('/delete/');
  const filename = deleteIndex !== -1 ? fullPath.substring(deleteIndex + '/delete/'.length) : '';
  const filePath = path.join(config.BOOKS_DIR, filename);

  try {
    if (fs.existsSync(filePath)) {
      await fs.promises.unlink(filePath);
    }
  } catch (err) {
    console.error(err);
  }

  return c.redirect('/admin');
});

app.post('/rename', async (c) => {
  const { oldFilename, newFilename } = await c.req.parseBody();

  if (oldFilename && newFilename) {
    const oldPath = path.join(config.BOOKS_DIR, oldFilename as string);
    const oldDir = path.dirname(oldPath);
    const ext = path.extname(oldFilename as string);

    let safeNew = newFilename as string;
    if (!path.extname(safeNew)) safeNew += ext;
    const newPath = path.join(oldDir, safeNew);

    try {
      if (fs.existsSync(oldPath) && !fs.existsSync(newPath)) {
        await fs.promises.rename(oldPath, newPath);
      }
    } catch (err) {
      console.error(err);
    }
  }

  return c.redirect('/admin');
});

app.all('*', async (c) => {
  const fullPath = c.req.path;

  if (c.req.method === 'GET') {
    let filename = '';

    if (fullPath.startsWith('/book/info/')) {
      filename = fullPath.substring('/book/info/'.length);
    } else if (fullPath.startsWith('/book/cover/')) {
      filename = fullPath.substring('/book/cover/'.length);
    } else if (fullPath.startsWith('/book/')) {
      filename = fullPath.substring('/book/'.length);
    } else if (fullPath.startsWith('/info/')) {
      filename = fullPath.substring('/info/'.length);
    } else if (fullPath.startsWith('/cover/')) {
      filename = fullPath.substring('/cover/'.length);
    } else if (fullPath.startsWith('/')) {
      filename = fullPath.substring('/'.length);
    }

    if (filename) {
      const filePath = path.join(config.BOOKS_DIR, filename);

      if (fullPath.startsWith('/book/info/') || fullPath.startsWith('/info/')) {
        if (!fs.existsSync(filePath)) return c.notFound();

        const bookInfo = await getBookInfo(filePath);
        const title = bookInfo?.title || path.basename(filename, path.extname(filename));
        const html = await renderView('book_details', {
          book: bookInfo || { title },
          filename: filename,
          title: title
        });
        return c.html(html);
      } else if (fullPath.startsWith('/book/cover/') || fullPath.startsWith('/cover/')) {
        if (!fs.existsSync(filePath)) return c.notFound();

        const bookInfo = await getBookInfo(filePath);
        if (bookInfo && bookInfo.cover) {
          c.header('Cache-Control', 'public, max-age=86400');
          c.header('Content-Type', 'image/jpeg');
          return c.body(bookInfo.cover as any);
        }
        return c.notFound();
      } else {
        if (fs.existsSync(filePath)) {
          const stat = fs.statSync(filePath);
          c.header('Content-Type', mime.lookup(filePath) || 'application/octet-stream');
          c.header('Content-Length', stat.size.toString());
          return c.body(fs.createReadStream(filePath) as any);
        }
      }
    }
  }

  return c.notFound();
});


export default app;
