import { setCookie } from "hono/cookie";
import { checkBasicAuth, checkCredentials, checkSession, createSessionToken, isAuthEnabled, SESSION_COOKIE } from "../helpers/auth";
import { renderView } from "../helpers/renderers";
import { Hono } from "hono";

const app = new Hono();

app.use("/login", async (c, next) => {
  if (!isAuthEnabled() || checkSession(c) || checkBasicAuth(c)) {
    return c.redirect('/admin');
  }

  return next();
});

app.get('/login', async (c) => {
  const html = await renderView('login', {
    title: 'Login - OPDShelf'
  });
  return c.html(html);
});

app.post('/login', async (c) => {
  const { username, password } = await c.req.parseBody();
  if (checkCredentials(username, password)) {
    // Set a session cookie that lasts 15 days
    setCookie(c, SESSION_COOKIE, createSessionToken(), { path: '/', httpOnly: true, maxAge: 1296000, sameSite: 'Lax' });
    return c.redirect('/admin');
  }
  return c.redirect('/user/login');
});

export default app;
