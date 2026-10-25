import { Context } from "hono";
import { getCookie } from "hono/cookie";
import { createHmac, timingSafeEqual } from "node:crypto";

export const SESSION_COOKIE = 'auth_session';

export function isAuthEnabled() {
  return Boolean(process.env.ADMIN_USERNAME && process.env.ADMIN_PASSWORD);
}

function safeEqual(a: string, b: string) {
  const bufA = Buffer.from(a);
  const bufB = Buffer.from(b);
  return bufA.length === bufB.length && timingSafeEqual(bufA, bufB);
}

export function checkCredentials(username: unknown, password: unknown) {
  if (!isAuthEnabled() || typeof username !== 'string' || typeof password !== 'string') return false;
  // Evaluate both comparisons so timing does not reveal which one failed
  const userOk = safeEqual(username, process.env.ADMIN_USERNAME!);
  const passOk = safeEqual(password, process.env.ADMIN_PASSWORD!);
  return userOk && passOk;
}

// Session token derived from the credentials: it survives restarts and
// is invalidated automatically when the username or password changes
export function createSessionToken() {
  return createHmac('sha256', process.env.ADMIN_PASSWORD!)
    .update(`opdshelf-session:${process.env.ADMIN_USERNAME}`)
    .digest('hex');
}

export function checkSession(c: Context) {
  const session = getCookie(c, SESSION_COOKIE);
  return Boolean(session) && isAuthEnabled() && safeEqual(session!, createSessionToken());
}

export function checkBasicAuth(c: Context) {
  const authHeader = c.req.header('Authorization');
  const match = authHeader?.match(/^Basic\s+(.*)$/);
  if (!match) return false;

  const credentials = Buffer.from(match[1], 'base64').toString();
  const splitIndex = credentials.indexOf(':');
  if (splitIndex === -1) return false;

  return checkCredentials(credentials.substring(0, splitIndex), credentials.substring(splitIndex + 1));
}
