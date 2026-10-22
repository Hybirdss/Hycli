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
    let response: Response;
    try { response = await fetch('/api/session', { cache: 'no-store', credentials: 'same-origin' }); }
    catch { throw new ApiError('connection_lost', 0); }
    if (response.status === 401) throw new ApiError('launch_required', 401);
    if (!response.ok) throw new ApiError('connection_lost', response.status);
    csrf = ((await response.json()) as { csrf: string }).csrf;
  })().finally(() => { renewing = null; });
  return renewing;
}
async function guardedFetch(path: string, method: string, body?: unknown, signal?: AbortSignal): Promise<Response> {
  for (let attempt = 0; attempt < 2; attempt++) {
    signal?.throwIfAborted();
    let response: Response;
    try {
      response = await fetch(path, {
        method, credentials: 'same-origin', cache: 'no-store', signal,
        headers: { 'Content-Type': 'application/json', 'X-Hycli-CSRF': csrf, 'Accept-Language': locale },
        body: body === undefined ? undefined : JSON.stringify(body),
      });
    } catch (error) { if (signal?.aborted) throw error; throw new ApiError('connection_lost', 0); }
    signal?.throwIfAborted();
    if (response.ok) return response;
    const data = await response.json().catch(() => ({}));
    signal?.throwIfAborted();
    // Only this middleware error proves the handler never ran; never replay an ambiguous write.
    if (attempt === 0 && response.status === 401 && data.error?.code === 'session_expired') { await session(); continue; }
    throw new ApiError(data.error?.code || 'internal', response.status);
  }
  throw new ApiError('connection_lost', 0);
}
export async function request<T>(path: string, method = 'GET', body?: unknown, signal?: AbortSignal): Promise<T> {
  const response = await guardedFetch(path, method, body, signal);
  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}
export async function download(path: string, name: string) {
  const response = await guardedFetch(path, 'GET');
  const url = URL.createObjectURL(await response.blob());
  const link = document.createElement('a'); link.href = url; link.download = name; link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
// HEAD has no error body. Renewal retries only this read-only availability check.
export async function checkArtifact(jobId: string, artifactId: string, signal: AbortSignal): Promise<void> {
  const path = artifactPath(jobId, artifactId);
  for (let attempt = 0; attempt < 2; attempt++) {
    let response: Response;
    try { response = await fetch(path, { method: 'HEAD', credentials: 'same-origin', cache: 'no-store', signal, headers: { 'Accept-Language': locale } }); }
    catch (error) { if (signal.aborted) throw error; throw new ApiError('connection_lost', 0); }
    signal.throwIfAborted();
    if (response.ok) return;
    if (attempt === 0 && response.status === 401) { await session(); signal.throwIfAborted(); continue; }
    throw new ApiError(response.status === 404 ? 'not_found' : [400, 409].includes(response.status) ? 'data_invalid' : response.status === 401 ? 'session_expired' : 'internal', response.status);
  }
}
function artifactPath(jobId: string, artifactId: string) {
  return `/api/jobs/${encodeURIComponent(jobId)}/artifacts/${encodeURIComponent(artifactId)}`;
}
export async function downloadArtifact(jobId: string, artifactId: string, name: string, signal: AbortSignal): Promise<void> {
  await checkArtifact(jobId, artifactId, signal);
  signal.throwIfAborted();
  const link = document.createElement('a');
  link.href = artifactPath(jobId, artifactId); link.download = name;
  document.body.appendChild(link);
  try { link.click(); } finally { link.remove(); }
}
export async function copyText(value: string) { await navigator.clipboard.writeText(value); }
