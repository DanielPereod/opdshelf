import { Hono } from 'hono';
import { serveStatic } from 'hono/bun';
import Handlebars from 'handlebars';
import { getConfig } from '../config';
import { getBooks, sortBooks, formatSize, getSimpleMime } from '../utils';
import { SortMode } from '../types';
import books from './book';
import user from './user';
import { renderView, renderXml } from '../helpers/renderers';
import { getBaseUrl } from '../helpers/general';
import { registerHandlebarsHelpers } from '../helpers/handlebars';
import { checkBasicAuth, checkSession, isAuthEnabled } from '../helpers/auth';

registerHandlebarsHelpers()
const app = new Hono();
const config = getConfig();

app.use('/static/*', serveStatic({ root: './' }));
app.use('*', async (c, next) => {
  if (c.req.path.startsWith('/user/') || c.req.path.startsWith('/static/') || !isAuthEnabled() || checkSession(c) || checkBasicAuth(c)) {
    return next();
  }

  // Handle denied
  const accept = c.req.header('Accept') || '';
  if (accept.includes('text/html')) {
    return c.redirect('/user/login');
  } else {
    c.header('WWW-Authenticate', 'Basic realm="OPDShelf"');
  }
  return c.text('Unauthorized', 401);
});

app.route("/book/", books);
app.route("/user", user);

app.get('/', async (c) => {
  const books = await getBooks(config.BOOKS_DIR);
  const sortMode = (c.req.query('sort') as SortMode) || 'date-desc';

  const xml = await renderXml('opds', {
    books: sortBooks(books, sortMode),
    baseUrl: getBaseUrl(c, config),
    currentTime: new Date().toISOString(),
    sortMode
  });

  c.header('Content-Type', 'application/atom+xml;charset=utf-8;profile=opds-catalog;kind=acquisition');
  return c.body(xml);
});

app.get('/admin', async (c) => {
  const books = await getBooks(config.BOOKS_DIR);
  const sortMode = (c.req.query('sort') as SortMode) || 'date-desc';

  const html = await renderView('admin', {
    books: sortBooks(books, sortMode),
    baseUrl: getBaseUrl(c, config),
    sortMode,
    title: 'OPDShelf Admin'
  });

  return c.html(html);
});

export default app;
