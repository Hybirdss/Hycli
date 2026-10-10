let csrf = '';
let locale = 'en';
let renewing: Promise<void> | null = null;
export class ApiError extends Error { constructor(public code: string, public status: number) { super(code); } }
export function websiteURL(value: string): string {
  const raw = value.trim();
  if (!raw || raw.length > 8192 || /[\u0000-\u001f\u007f]/.test(raw)) throw new ApiError('bad_url', 400);
  try {
    const url = new URL(raw.startsWith('//') ? `https:${raw}` : raw.includes('://') ? raw : `https://${raw}`);
    if (!['https:', 'http:'].includes(url.protocol) || url.username || url.password) throw new Error('Invalid website');
    url.hash = '';
    return url.href;
  } catch { throw new ApiError('bad_url', 400); }
}
export function setApiLocale(value: string) { locale = value; }
export function session(): Promise<void> {
  if (renewing) return renewing;
  renewing = (async () => {
    // The launch address from `hycli dashboard` grants this browser its session once.
    const launch = new URLSearchParams(location.search).get('launch');
    let response: Response;
    try { response = await fetch(launch ? `/api/session?launch=${encodeURIComponent(launch)}` : '/api/session', { cache: 'no-store', credentials: 'same-origin' }); }
    catch { throw new ApiError('connection_lost', 0); }
    if (response.status === 401) throw new ApiError('launch_required', 401);
    if (!response.ok) throw new ApiError('connection_lost', response.status);
    csrf = ((await response.json()) as { csrf: string }).csrf;
    if (launch) history.replaceState(null, '', location.pathname + location.hash);
  })().finally(() => { renewing = null; });
  return renewing;
}
async function guardedFetch(path: string, method: string, body?: unknown): Promise<Response> {
  for (let attempt = 0; attempt < 2; attempt++) {
    let response: Response;
    try {
      response = await fetch(path, {
        method, credentials: 'same-origin', cache: 'no-store',
        headers: { 'Content-Type': 'application/json', 'X-Hycli-CSRF': csrf, 'Accept-Language': locale },
        body: body === undefined ? undefined : JSON.stringify(body),
      });
    } catch { throw new ApiError('connection_lost', 0); }
    if (response.ok) return response;
    const data = await response.json().catch(() => ({}));
    // Only this middleware error proves the handler never ran; never replay an ambiguous write.
    if (attempt === 0 && response.status === 401 && data.error?.code === 'session_expired') { await session(); continue; }
    throw new ApiError(data.error?.code || 'internal', response.status);
  }
  throw new ApiError('connection_lost', 0);
}
export async function request<T>(path: string, method = 'GET', body?: unknown): Promise<T> {
  const response = await guardedFetch(path, method, body);
  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}
export async function download(path: string, name: string) {
  const response = await guardedFetch(path, 'GET');
  const url = URL.createObjectURL(await response.blob());
  const link = document.createElement('a'); link.href = url; link.download = name; link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
export async function copyText(value: string) { await navigator.clipboard.writeText(value); }
