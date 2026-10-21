import { useEffect, useRef, useState } from 'react';
import { ArrowDownToLine, Search, ArrowRight, Check, CheckCircle2, Copy, ExternalLink, Globe2, KeyRound, Link2, LoaderCircle, Plus, RefreshCw, ShieldCheck, Sparkles, Trash2, UserRound } from 'lucide-react';
import { ApiError, copyText, download, request, websiteURL } from './api';
import { useI18n } from './i18n';
import { FileResult, fileArtifact } from './file-result';
import { Badge, Bird, Button, ErrorNotice, Field, IconButton, Modal, ProviderMark, SearchField, Select, SiteIcon, Toggle } from './ui';
import { Progress, WorkNotes, isWorking, providerName } from './work';
import type { Operations } from './App';
import type { Action, Approval, Job, Provider, Site, Snapshot } from './types';

function codeOf(e: unknown) { return e instanceof ApiError ? e.code : 'internal'; }
function sameOrigin(first: string, second: string) { try { return new URL(websiteURL(first)).origin === new URL(websiteURL(second)).origin; } catch { return false; } }
function formatValue(value: unknown) { return typeof value === 'string' ? value : JSON.stringify(value, null, 2); }
function WebsiteKeys({ site, ops, only }: { site: Site; ops: Operations; only?: string }) {
  const { t } = useI18n();
  const [values, setValues] = useState<Record<string, string>>({}); const [busy, setBusy] = useState(''); const [error, setError] = useState('');
  const keyLabel = (key: {id: string; label: string}) => /^website-key-[0-9a-f]{8}$/.test(key.id) ? t('connections.key') : key.label;
  const keys = (site.credentials || []).filter(key => !only || key.id === only);
  if (!keys.length) return null;
  const save = async (id: string, remove = false) => {
    setBusy(id); setError('');
    try { await ops.act(`/api/sites/${site.id}/credentials/${encodeURIComponent(id)}`, remove ? 'DELETE' : 'PUT', remove ? undefined : { key: values[id] }); setValues(previous => ({...previous, [id]: ''})); ops.toast('settings.saved'); }
    catch (e) { setError(codeOf(e)); } finally { setBusy(''); }
  };
  return <section className="website-keys"><h3><KeyRound size={17} />{t('accounts.websiteKeys')}</h3><p className="subtle-note">{t('accounts.websiteKeyHint', { site: site.title })}</p>{keys.map(key => <Field key={key.id} label={keyLabel(key)}><div className="inline-input"><input aria-label={`${keyLabel(key)} · ${t('connections.key')}`} type="password" autoComplete="off" maxLength={16384} value={values[key.id] || ''} placeholder={key.configured ? '••••••••••••' : t('connections.key')} onChange={e => setValues(previous => ({ ...previous, [key.id]: e.target.value }))} /><Button type="button" variant="secondary" loading={busy === key.id} disabled={!values[key.id]?.trim()} onClick={() => { void save(key.id); }}>{t('common.save')}</Button>{key.configured && <IconButton type="button" label={t('common.remove')} disabled={busy === key.id} onClick={() => { void save(key.id, true); }}><Trash2 size={16} /></IconButton>}</div>{key.configured && <small className="identity-verified"><ShieldCheck size={12} />{t('accounts.keySaved')}</small>}</Field>)}{error && <ErrorNotice code={error} />}</section>;
}
export function WebsiteDialog({ site, data, ops, onClose, run, connectAccount, remove }: { site: Site; data: Snapshot; ops: Operations; onClose: () => void; run: (a: Action) => void; connectAccount: () => void; remove: () => void }) {
  const { t, date } = useI18n(); const [search, setSearch] = useState(''); const [title, setTitle] = useState(site.title); const [error, setError] = useState(''); const [busy, setBusy] = useState(false);
  const actions = site.actions.filter(a => `${a.title} ${a.description}`.toLocaleLowerCase().includes(search.toLocaleLowerCase()));
  const update = async (path: string, body?: unknown) => { setBusy(true); setError(''); try { await ops.act(path, 'POST', body); } catch (e) { setError(codeOf(e)); } finally { setBusy(false); } };
  return <Modal title={site.title} subtitle={site.host} onClose={onClose} wide><div className="website-detail-hero"><SiteIcon title={site.title} image={site.image_url} size="large" /><div><p>{site.summary}</p><a href={site.url} target="_blank" rel="noreferrer">{t('sites.openWebsite')}<ExternalLink size={13} /></a></div></div>{error && <ErrorNotice code={error} />}<WebsiteKeys site={site} ops={ops} />{data.jobs.filter(j => j.site_id === site.id && isWorking(j)).map(job => <Progress key={job.id} job={job} compact onCancel={() => { void ops.act(`/api/jobs/${job.id}/cancel`).catch(e => setError(codeOf(e))); }} />)}<div className="detail-section-heading"><h3>{t('actions.title')}</h3><SearchField value={search} onChange={setSearch} placeholder={t('actions.search')} /></div><div className="action-list">{actions.map(action => <button className="action-row" key={action.id} onClick={() => run(action)}><span className={`action-type action-type--${action.effect}`}>{action.effect === 'read' ? <Globe2 size={18} /> : <ShieldCheck size={18} />}</span><span className="action-row-text"><strong>{action.title}</strong><span>{action.description}</span><small>{t(action.effect === 'read' ? 'safety.readOnly' : action.effect === 'write' ? 'actions.write' : 'safety.unknownEffect')}{action.effect !== 'read' && !site.writes && <> · {t('sites.changesOff')}</>}{action.verified && <> · <Check size={11} /> {t('actions.verified')}</>}</small></span><ArrowRight size={17} /></button>)}{!actions.length && <p className="list-empty">{t('actions.noResults')}</p>}</div><p className="subtle-note"><ShieldCheck size={14} />{t('sites.readinessNote')}</p><Toggle label={t('sites.allowChanges')} description={t('sites.allowChangesHint')} value={site.writes} onChange={v => { void ops.act(`/api/sites/${site.id}`, 'PATCH', { writes: v }).catch(e => setError(codeOf(e))); }} /><div className="website-detail-settings"><Field label={t('sites.titleLabel')}><div className="inline-input"><input aria-label={t('sites.titleLabel')} value={title} onChange={e => setTitle(e.target.value)} maxLength={100} /><Button variant="secondary" disabled={title === site.title || !title.trim()} onClick={() => { void ops.act(`/api/sites/${site.id}`, 'PATCH', { title }).catch(e => setError(codeOf(e))); }}>{t('common.save')}</Button></div></Field><Field label={t('sites.authLabel')} hint={t('sites.authDescription')}><div className="inline-input"><Select value={site.account_id || ''} aria-label={t('accounts.choose')} onChange={e => { void ops.act(`/api/sites/${site.id}`, 'PATCH', { account_id: e.target.value }).catch(e => setError(codeOf(e))); }}><option value="">{t(site.credentials?.length ? 'browser.notConnected' : 'sites.publicAccess')}</option>{data.accounts.filter(a => a.site_id === site.id || (a.verified_identity && a.status === 'connected' && sameOrigin(a.site_url, site.url))).map(a => <option value={a.id} key={a.id}>{a.label || `${a.browser} · ${a.profile || t('accounts.unknown')}`}</option>)}</Select><Button variant="secondary" onClick={connectAccount}><Plus size={15} />{t('accounts.add')}</Button></div></Field></div><div className="detail-management"><Button variant="secondary" loading={busy} onClick={() => { void update('/api/sites/prepare', { site_id: site.id, url: site.url, provider: data.settings.default_provider }); }}><RefreshCw size={14} />{t('sites.prepareAgain')}</Button><Button variant="secondary" loading={busy} onClick={() => { void update(`/api/sites/${site.id}/describe`, { provider: data.settings.default_provider }); }}><Sparkles size={14} />{t('sites.describe')}</Button><Button variant="ghost" onClick={() => { void download(`/api/sites/${site.id}/export`, `${site.id}.yaml`).catch(e => setError(codeOf(e))); }}><ArrowDownToLine size={15} />{t('sites.export')}</Button><IconButton label={t('common.remove')} onClick={remove}><Trash2 size={16} /></IconButton></div><p className="last-updated">{t('common.lastUpdated', { date: date(site.updated_at) })}</p></Modal>;
}
export function ActionDialog({ site, action, data, ops, onClose, onJob, onApproval }: { site: Site; action: Action; data: Snapshot; ops: Operations; onClose: () => void; onJob: (job: Job) => void; onApproval: (a: Approval) => void }) {
  const accounts = data.accounts.filter(a => (a.site_id === site.id || a.id === site.account_id) && a.status === 'connected' && sameOrigin(a.site_url, site.url));
  const { t } = useI18n(); const [values, setValues] = useState<Record<string, string>>(() => Object.fromEntries(action.inputs.filter(f => f.default !== undefined && f.default !== null).map(f => [f.id, typeof f.default === 'object' ? JSON.stringify(f.default, null, 2) : String(f.default)]))); const [account, setAccount] = useState(action.accepts_account !== false && accounts.some(a => a.id === site.account_id) ? site.account_id || '' : ''); const [error, setError] = useState(''); const [busy, setBusy] = useState(false);
  const submit = async (e: React.FormEvent) => { e.preventDefault(); setBusy(true); setError(''); try { const result = await ops.act<{ job?: Job; approval?: Approval }>(`/api/sites/${site.id}/actions/${encodeURIComponent(action.id)}`, 'POST', { inputs: Object.fromEntries(Object.entries(values).filter(([key, value]) => value !== '' || action.inputs.find(f => f.id === key)?.required)), account_id: account }); if (result.approval) onApproval(result.approval); else if (result.job) onJob(result.job); else setError('internal'); } catch (e) { setError(codeOf(e)); } finally { setBusy(false); } };
  return <Modal title={action.title} subtitle={site.title} onClose={onClose}><p className="dialog-intro">{action.description}</p><div className="action-summary"><Badge tone={action.effect === 'read' ? 'green' : 'amber'}>{t(action.effect === 'read' ? 'safety.readOnly' : action.effect === 'write' ? 'actions.write' : 'safety.unknownEffect')}</Badge>{!action.verified && <span>{t('actions.unverified')}</span>}</div>{action.output && <div className="output-note"><span>{t('actions.output')}</span><p>{action.output}</p></div>}<form onSubmit={submit} className="dialog-form">{action.credential_id && <WebsiteKeys site={data.sites.find(current => current.id === site.id) || site} ops={ops} only={action.credential_id} />}{action.accepts_account !== false && <Field label={t('safety.account')}><Select value={account} onChange={e => setAccount(e.target.value)}><option value="">{t(site.credentials?.length ? 'browser.notConnected' : 'sites.publicAccess')}</option>{accounts.map(a => <option value={a.id} key={a.id}>{a.label || `${a.browser} · ${a.profile || t('accounts.unknown')}`}</option>)}</Select></Field>}{action.inputs.map(field => <Field key={field.id} label={`${field.label}${field.required ? '' : ` · ${t('common.optional')}`}`} hint={field.type === 'file' ? [field.hint, t('actions.filePathHint')].filter(Boolean).join(' ') : field.hint}>{field.type === 'bool' ? <Select value={values[field.id] || ''} required={field.required} onChange={e => setValues(v => ({ ...v, [field.id]: e.target.value }))}><option value="">—</option><option value="true">{t('common.yes')}</option><option value="false">{t('common.no')}</option></Select> : field.type === 'json' ? <textarea rows={5} required={field.required} value={values[field.id] ?? ''} onChange={e => setValues(v => ({ ...v, [field.id]: e.target.value }))} spellCheck={false} /> : <input type={['int', 'float'].includes(field.type) ? 'number' : 'text'} step={field.type === 'float' ? 'any' : field.type === 'int' ? 1 : undefined} required={field.required} value={values[field.id] ?? ''} onChange={e => setValues(v => ({ ...v, [field.id]: e.target.value }))} placeholder={t('actions.fieldPlaceholder', { field: field.label })} />}</Field>)}{!action.inputs.length && <p className="subtle-note">{t('actions.noInputs')}</p>}{error && <ErrorNotice code={error} />}<div className="dialog-buttons"><Button variant="secondary" type="button" onClick={onClose}>{t('common.cancel')}</Button><Button type="submit" loading={busy}>{t(action.effect === 'read' ? 'actions.run' : 'actions.review')}<ArrowRight size={15} /></Button></div></form></Modal>;
}
export function ApprovalDialog({ approval, ops, onClose, onJob, onResolved }: { approval: Approval; ops: Operations; onClose: () => void; onResolved: () => void; onJob: (job: Job) => void }) {
  const { t } = useI18n(); const [error, setError] = useState(''); const [busy, setBusy] = useState(false); const [expired, setExpired] = useState(new Date(approval.expires_at).getTime() <= Date.now());
  useEffect(() => { const timer = window.setInterval(() => setExpired(new Date(approval.expires_at).getTime() <= Date.now()), 1000); return () => clearInterval(timer); }, [approval.expires_at]);
  return <Modal title={t('safety.changeTitle')} subtitle={t('safety.changeDescription')} onClose={onClose}><div className="approval-shield"><ShieldCheck size={28} /></div><dl className="review-details"><div><dt>{t('accounts.website')}</dt><dd>{approval.site_title}</dd></div><div><dt>{t('safety.account')}</dt><dd>{approval.account_label || t('sites.publicAccess')}</dd></div><div><dt>{t('safety.action')}</dt><dd>{approval.action_title}</dd></div>{approval.inputs.map((input, i) => <div key={i}><dt>{input.label}</dt><dd>{input.value}</dd></div>)}</dl>{expired && <ErrorNotice code="approval_expired" />}{error && <ErrorNotice code={error} />}<div className="dialog-buttons"><Button variant="secondary" disabled={busy} onClick={() => { setBusy(true); void ops.act(`/api/approvals/${approval.id}/reject`).then(onResolved).catch(e => setError(codeOf(e))).finally(() => setBusy(false)); }}>{t('common.cancel')}</Button><Button disabled={expired} loading={busy} onClick={() => { setBusy(true); void ops.act<{ job: Job }>(`/api/approvals/${approval.id}/approve`).then(r => onJob(r.job)).catch(e => setError(codeOf(e))).finally(() => setBusy(false)); }}>{t('safety.allowOnce')}<ArrowRight size={15} /></Button></div></Modal>;
}
export function ProviderDialog({ provider: initialProvider, data, ops, onClose }: { provider: Provider; data: Snapshot; ops: Operations; onClose: () => void }) {
  const [providerID, setProviderID] = useState(initialProvider.id);
  const provider = data.providers.find(p => p.id === providerID) || initialProvider;
  const { t } = useI18n(); const [key, setKey] = useState(''); const [model, setModel] = useState(provider.model || provider.default_model); const [defaultProvider, setDefaultProvider] = useState(data.settings.default_provider === provider.id || !data.settings.default_provider); const [error, setError] = useState(''); const [busy, setBusy] = useState(false); const [authURL, setAuthURL] = useState(''); const [loginID, setLoginID] = useState('');
  const chooseProvider = (id: string) => {
    const next = data.providers.find(p => p.id === id);
    if (!next) return;
    setProviderID(id);
    setKey('');
    setModel(next.model || next.default_model);
    setDefaultProvider(data.settings.default_provider === id || !data.settings.default_provider);
    setError('');
  };
  const submit = async (e: React.FormEvent) => { e.preventDefault(); setBusy(true); setError(''); try { await ops.act(`/api/providers/${provider.id}`, 'PUT', { key: key || undefined, model, default: defaultProvider }); setKey(''); ops.toast('connections.testSuccess'); onClose(); } catch (e) { setError(codeOf(e)); } finally { setBusy(false); } };
  useEffect(() => { if (!loginID) return; const timer = window.setInterval(() => { void request<{ connected: boolean }>('/api/providers/codex/account').then(async r => { if (r.connected) { clearInterval(timer); await ops.refresh(); setLoginID(''); setAuthURL(''); ops.toast('connections.codexReady'); } }).catch(() => {}); }, 2500); return () => clearInterval(timer); }, [loginID, ops]);
  const signIn = async () => { setBusy(true); setError(''); try { const result = await request<{ auth_url: string; login_id: string }>('/api/providers/codex/login', 'POST'); setAuthURL(result.auth_url); setLoginID(result.login_id); } catch (e) { setError(codeOf(e)); } finally { setBusy(false); } };
  return <Modal title={['codex', 'openai'].includes(provider.id) ? 'ChatGPT' : provider.id === 'anthropic' ? 'Claude' : provider.name} subtitle={t('connections.subtitle')} onClose={onClose}><form onSubmit={submit} className="dialog-form"><div className="provider-dialog-mark"><ProviderMark id={provider.id} /></div>{['codex', 'openai'].includes(provider.id) && <div className="segmented-control connection-methods" aria-label={t('connections.method')}>{['codex','openai'].map(id => <button type="button" key={id} className={provider.id === id ? 'selected' : ''} aria-pressed={provider.id === id} disabled={busy} onClick={() => chooseProvider(id)}>{id === 'codex' ? <UserRound size={16} /> : <KeyRound size={16} />}{t(id === 'codex' ? 'connections.login' : 'connections.key')}</button>)}</div>}{['zai', 'zai-coding-plan'].includes(provider.id) && <Field label={t('connections.provider')}><Select aria-label={t('connections.provider')} value={provider.id} onChange={e => chooseProvider(e.target.value)} disabled={busy}>{data.providers.filter(p => ['zai', 'zai-coding-plan'].includes(p.id)).map(p => <option key={p.id} value={p.id}>{p.name}</option>)}</Select></Field>}{provider.id === 'codex' ? <div className="codex-connect"><div className="connection-illustration"><Bird /><span><UserRound size={26} /></span></div><p>{t('connections.codexDescription')}</p>{provider.connected ? <div className="notice notice--success"><CheckCircle2 size={18} />{provider.account || t('connections.codexReady')}</div> : !provider.available ? <div className="notice">{t('connections.codexMissing')}<a href="https://developers.openai.com/codex/cli" target="_blank" rel="noreferrer">{t('common.learnMore')}<ExternalLink size={12} /></a></div> : <><Button type="button" loading={busy} onClick={() => { void signIn(); }}>{t('connections.codexSignIn')}<ArrowRight size={15} /></Button><Button type="button" variant="ghost" onClick={() => { setBusy(true); void ops.act('/api/providers/codex/check').then(() => ops.toast('connections.testSuccess')).catch(e => setError(codeOf(e))).finally(() => setBusy(false)); }}>{t('connections.codexExisting')}</Button></>}{authURL && <div className="notice notice--signin"><p>{t('connections.codexWaiting')}</p><a className="button button--secondary" href={authURL} target="_blank" rel="noreferrer">{t('connections.codexContinue')}<ExternalLink size={15} /></a></div>}</div> : <><Field label={t('connections.key')} hint={provider.has_key ? t('connections.keySaved') : t('connections.keyHint')}><input type="password" autoComplete="off" value={key} onChange={e => setKey(e.target.value)} required={!provider.has_key} placeholder={provider.has_key ? '••••••••••••••••' : t('connections.keyPlaceholder')} spellCheck={false} /></Field>{provider.key_url && <a className="provider-key-link" href={provider.key_url} target="_blank" rel="noreferrer">{t('connections.openProvider')}<ExternalLink size={13} /></a>}</>}
    <Field label={t('connections.model')} hint={t('connections.modelHint')}><input value={model} list={`models-${provider.id}`} onChange={e => setModel(e.target.value)} placeholder={provider.default_model || t('connections.customModel')} autoComplete="off" spellCheck={false} /><datalist id={`models-${provider.id}`}>{provider.models.map(name => <option value={name} key={name} />)}</datalist></Field><label className="checkbox-label"><input type="checkbox" checked={defaultProvider} onChange={e => setDefaultProvider(e.target.checked)} /><span>{t('connections.default')}</span></label>{error && <ErrorNotice code={error} />}<p className="subtle-note">{t('connections.usageNote')}</p><div className="dialog-buttons">{provider.connected && <Button type="button" variant="ghost" onClick={() => { setBusy(true); void ops.act(`/api/providers/${provider.id}`, 'DELETE').then(onClose).catch(e => setError(codeOf(e))).finally(() => setBusy(false)); }}>{t('connections.disconnect')}</Button>}<Button type="button" variant="secondary" onClick={onClose}>{t('common.cancel')}</Button><Button type="submit" disabled={provider.id === 'codex' && !provider.connected} loading={busy}>{t('common.save')}</Button></div></form></Modal>;
}
export function AccountDialog({ site, data, ops, onClose, resume }: { site: Site | null; data: Snapshot; ops: Operations; onClose: () => void; resume?: { url: string; provider?: string; intent?: string } | null }) {
  const { t } = useI18n(); const [selected, setSelected] = useState(site?.id || (resume ? '' : data.sites[0]?.id || '')); const [url, setURL] = useState(site?.url || resume?.url || ''); const [pair, setPair] = useState<{ code: string; expires_at: string } | null>(null); const [error, setError] = useState(''); const [busy, setBusy] = useState(false); const [tab, setTab] = useState('auto'); const file = useRef<HTMLInputElement>(null); const before = useRef(new Set(data.accounts.map(a => a.id)));
  type SearchResult = { profiles_checked: number; profiles_unavailable: number; accounts: Snapshot['accounts']; selected_account: string; needs_choice: boolean; login_url: string };
  const [found, setFound] = useState<SearchResult | null>(null); const [waiting, setWaiting] = useState(false); const scanning = useRef(false); const resuming = useRef(false);
  const nativeRequest = useRef<AbortController | null>(null);
  const nativeEnabled = data.settings.native_browser_discovery === true;
  const savingNative = useRef(false);
  const saveNative = async (enabled: boolean) => {
    if (savingNative.current) return;
    savingNative.current = true; setError('');
    try { await ops.act('/api/settings', 'PATCH', { native_browser_discovery: enabled }); }
    catch (error) { if (currentTarget.current === targetKey) setError(codeOf(error)); }
    finally { savingNative.current = false; }
  };
  const targetURL = selected ? data.sites.find(s => s.id === selected)?.url || '' : url;
  const targetIdentity = JSON.stringify([selected, targetURL]);
  const targetRevision = useRef({ identity: targetIdentity, revision: 0 });
  if (targetRevision.current.identity !== targetIdentity) targetRevision.current = { identity: targetIdentity, revision: targetRevision.current.revision + 1 };
  const targetKey = JSON.stringify([targetIdentity, targetRevision.current.revision]);
  const currentTarget = useRef(targetKey); currentTarget.current = targetKey;
  const pendingContinuation = useRef<{ id: string; target: string } | null>(null);
  const attemptedContinuations = useRef(new Set<string>());
  const [imported, setImported] = useState<Snapshot['accounts'][number] | null>(null);
  useEffect(() => { currentTarget.current = targetKey; return () => { currentTarget.current = ''; pendingContinuation.current = null; }; }, [targetKey]);
  const resumeWith = async (id: string, explicitChoice = false) => {
    if (currentTarget.current !== targetKey || resuming.current) return;
    if (!explicitChoice) {
      if (attemptedContinuations.current.has(JSON.stringify([targetKey, id]))) return;
      const snapshot = ops.freshSnapshot();
      if (!snapshot) { pendingContinuation.current = { id, target: targetKey }; return; }
      const account = snapshot.accounts.find(a => a.id === id);
      if (!account || !account.verified_identity || account.status !== 'connected' || !sameOrigin(account.site_url, targetURL) || (selected && account.site_id !== selected)) return;
    }
    pendingContinuation.current = null;
    attemptedContinuations.current.add(JSON.stringify([targetKey, id]));
    resuming.current = true; setWaiting(false);
    try {
      await ops.act('/api/browser/select', 'POST', { account_id: id, url: targetURL, ...(explicitChoice && selected ? { site_id: selected } : {}) });
      if (currentTarget.current !== targetKey) return;
      if (selected || resume) await ops.act('/api/sites/prepare', 'POST', { site_id: selected, url: targetURL, provider: resume?.provider || data.settings.default_provider, intent: resume?.intent });
      if (currentTarget.current !== targetKey) return;
      ops.toast('accounts.connectedToast'); onClose();
    } finally { resuming.current = false; }
  };
  const continuePending = () => {
    const pending = pendingContinuation.current;
    if (!pending || pending.target !== targetKey || !ops.freshSnapshot()) return;
    pendingContinuation.current = null;
    void resumeWith(pending.id).catch(e => setError(codeOf(e)));
  };
  useEffect(continuePending, [data, targetKey, ops]);
  const importAccount = async (content: string) => {
    const { account } = await ops.act<{ account: Snapshot['accounts'][number] }>('/api/accounts/import', 'POST', { site_id: selected, url: targetURL, content });
    if (currentTarget.current !== targetKey) return;
    setImported(account);
    await resumeWith(account.id);
  };
  const scan = async (resume = false) => {
    if (scanning.current || !targetURL || !nativeEnabled) return;
    const controller = new AbortController(); nativeRequest.current = controller;
    scanning.current = true; setBusy(true); setError('');
    try {
      const result = await ops.act<SearchResult>('/api/browser/find', 'POST', { site_id: selected, url: targetURL }, controller.signal);
      if (controller.signal.aborted || currentTarget.current !== targetKey) return;
      if (result.needs_choice) pendingContinuation.current = null;
      setFound(result);
      if (resume && !result.needs_choice && result.selected_account && result.accounts.some(a => a.id === result.selected_account && a.verified_identity && a.status === 'connected')) await resumeWith(result.selected_account);
    } catch (error) { if (!controller.signal.aborted && currentTarget.current === targetKey) setError(codeOf(error)); } finally { if (nativeRequest.current === controller) { nativeRequest.current = null; scanning.current = false; setBusy(false); } }
  };
  useEffect(() => () => { nativeRequest.current?.abort(); nativeRequest.current = null; scanning.current = false; }, [targetKey]);
  useEffect(() => { setBusy(false); pendingContinuation.current = null; attemptedContinuations.current.clear(); setImported(null); setFound(null); setWaiting(false); setPair(null); }, [selected, resume?.url, url]);
  useEffect(() => {
    if (!waiting) return;
    const snapshot = ops.freshSnapshot();
    if (!snapshot) return;
    const candidates = snapshot.accounts.filter(a => (selected ? a.site_id === selected : sameOrigin(a.site_url, targetURL)) && a.verified_identity && a.status === 'connected');
    const chosen = snapshot.sites.find(s => s.id === selected)?.account_id || (!found?.needs_choice ? found?.selected_account : undefined);
    const account = chosen ? candidates.find(a => a.id === chosen) : candidates.length === 1 && !found?.needs_choice ? candidates[0] : undefined;
    if (account) void resumeWith(account.id).catch(e => setError(codeOf(e)));
  }, [data, waiting, found, targetKey]);
  const openLogin = async () => { setError(''); try { await ops.act('/api/browser/login', 'POST', { site_id: selected, url: targetURL }); if (currentTarget.current !== targetKey) return; setWaiting(true); } catch (e) { setError(codeOf(e)); } };
  const [profiles, setProfiles] = useState<{id:string;browser:string;profile:string}[]>([]);
  useEffect(() => { void request<{profiles:{id:string;browser:string;profile:string}[]}>('/api/browser/discovery').then(result => setProfiles(result.profiles)).catch(() => {}); }, []);
  useEffect(() => {
    if (!pair || !targetURL) return;
    const snapshot = ops.freshSnapshot();
    const account = snapshot?.accounts.find(a => !before.current.has(a.id) && sameOrigin(a.site_url, targetURL) && a.status === 'connected' && a.verified_identity && (!selected || a.site_id === selected));
    if (account) void resumeWith(account.id).catch(e => setError(codeOf(e)));
  }, [data, pair, targetKey]);
  const connect = async (e: React.FormEvent) => { e.preventDefault(); if (tab === 'auto') { await scan(waiting); return; } if (tab !== 'companion') return; setBusy(true); setError(''); try { const result = await ops.act<{ code: string; expires_at: string }>('/api/accounts/pair', 'POST', { site_id: selected, url: selected ? data.sites.find(s => s.id === selected)?.url : url }); if (currentTarget.current !== targetKey) return; setPair(result); } catch (e) { setError(codeOf(e)); } finally { setBusy(false); } };
  return <Modal title={t('accounts.add')} subtitle={t('accounts.subtitle')} onClose={onClose}><div className="connection-illustration connection-illustration--browser"><img src="/brand/browser-companion.png" alt="" /></div><div className="privacy-note"><ShieldCheck size={17} /><p>{t('accounts.noSecretAI')}</p></div>{profiles.length > 0 && <div className="browser-discovery"><h3>{t('accounts.alreadySignedIn')}</h3><div>{profiles.map(profile => <span key={profile.id}><Globe2 size={14} />{profile.browser}<small>{profile.profile}</small></span>)}</div></div>}<div className="filter-tabs account-tabs"><button onClick={() => setTab('auto')} className={tab === 'auto' ? 'selected' : ''}>{t('accounts.autoFind')}</button><button onClick={() => setTab('companion')} className={tab === 'companion' ? 'selected' : ''}>{t('accounts.companion')}</button><button onClick={() => setTab('import')} className={tab === 'import' ? 'selected' : ''}>{t('browser.import')}</button></div><form onSubmit={connect} className="dialog-form"><Field label={t('accounts.website')}><Select value={selected} onChange={e => { setSelected(e.target.value); setPair(null); }}><option value="">{t('sites.add')}</option>{data.sites.map(s => <option value={s.id} key={s.id}>{s.title}</option>)}</Select></Field>{!selected && <Field label={t('sites.urlLabel')}><input type="text" inputMode="url" required value={url} onChange={e => setURL(e.target.value)} placeholder="https://example.com" /></Field>}{tab === 'auto' ? <section className="auto-signin"><Toggle label={t('settings.nativeDiscovery')} description={t('settings.nativeDiscoveryHint')} value={nativeEnabled} onChange={enabled => { void saveNative(enabled); }} /><p>{t('accounts.autoFindHint')}</p><Button type="button" loading={busy} disabled={!targetURL || !nativeEnabled} onClick={() => { void scan(waiting); }}>{t(busy ? 'accounts.searching' : found ? 'accounts.recheck' : 'accounts.autoFind')}<Search size={16} /></Button>{found && <><p className="subtle-note">{t('accounts.searchDone', { count: found.profiles_checked })}</p><p>{t(found.needs_choice ? 'accounts.chooseFound' : found.accounts.some(a => a.status === 'connected' && a.verified_identity) ? 'accounts.connectedToast' : found.accounts.some(a => a.status === 'connected') ? 'accounts.sessionFound' : 'accounts.noSession')}</p>{found.accounts.filter(a => a.status === 'connected').map(account => <div className="found-account" key={account.id}><UserRound size={18} /><div><strong>{account.label || account.browser}</strong><small>{account.profile}{account.label ? ` · ${account.browser}` : ''}</small></div><Button type="button" variant="secondary" onClick={() => { void resumeWith(account.id, true).catch(e => setError(codeOf(e))); }}>{t('accounts.useAccount')}</Button></div>)}</>}<Button type="button" variant="secondary" disabled={!targetURL} onClick={() => { void openLogin(); }}>{t('accounts.openLogin')}<ExternalLink size={15} /></Button>{waiting && <p className="pairing-wait"><LoaderCircle className="spin" size={15} />{t('accounts.waitLogin')}</p>}{waiting && <p className="subtle-note">{t('accounts.loginPending')}</p>}</section> : tab === 'companion' ? <><p className="dialog-intro">{t('accounts.companionHint')}</p>{pair ? <div className="pairing-box"><span>{t('accounts.pairingCode')}</span><div><code>{pair.code}</code><IconButton label={t('accounts.copyPairing')} onClick={() => { void copyText(pair.code).then(() => ops.toast('common.copied')).catch(() => setError('internal')); }} type="button"><Copy size={19} /></IconButton></div><p>{t('accounts.pairingExpires')}</p><span className="pairing-wait"><LoaderCircle className="spin" size={14} />{t('accounts.waiting')}</span></div> : <Button type="submit" loading={busy}>{t('browser.connect')}<ArrowRight size={15} /></Button>}<div className="companion-links"><button type="button" onClick={() => { void download('/api/browser/extension', 'hycli-browser-companion.zip').catch(e => setError(codeOf(e))); }}><ArrowDownToLine size={14} />{t('accounts.downloadCompanion')} · Chrome / Edge</button><button type="button" onClick={() => { void download('/api/browser/extension?browser=firefox', 'hycli-firefox-companion.zip').catch(e => setError(codeOf(e))); }}><ArrowDownToLine size={14} />Firefox</button><a href="/guide/browser" target="_blank" rel="noreferrer">{t('accounts.setupGuide')}<ExternalLink size={13} /></a></div></> : <><p className="dialog-intro">{t('browser.importHint')}</p><input ref={file} type="file" accept=".json,.txt" className="visually-hidden" onChange={e => { const selectedFile = e.target.files?.[0]; e.target.value = ''; if (!selectedFile) return; if (selectedFile.size > 1024 * 1024) { setError('too_large'); return; } setBusy(true); void selectedFile.text().then(content => { if (currentTarget.current === targetKey) return importAccount(content); }).catch(e => setError(codeOf(e))).finally(() => setBusy(false)); }} /><Button type="button" loading={busy} disabled={!selected && !url} onClick={() => file.current?.click()}><Plus size={15} />{t('browser.importFile')}</Button>{imported && <div className="found-account"><UserRound size={18} /><div><strong>{imported.label || imported.browser}</strong><small>{t('accounts.sessionFound')}</small></div></div>}</>}{error && <ErrorNotice code={error} />}</form><p className="subtle-note">{t('browser.privacy')}</p></Modal>;
}
export function HandoffDialog({ site, ops, onClose }: { site: Site | null; ops: Operations; onClose: () => void }) {
  const { t } = useI18n(); const [tab, setTab] = useState('config'); const [content, setContent] = useState<{ instructions: string; config: unknown; command: string } | null>(null); const [error, setError] = useState('');
  useEffect(() => { void request<{ instructions: string; config: unknown; command: string }>(`/api/handoff${site ? `?site=${encodeURIComponent(site.id)}` : ''}`).then(setContent).catch(e => setError(codeOf(e))); }, [site]);
  const value = content ? tab === 'instructions' ? content.instructions : tab === 'config' ? JSON.stringify(content.config, null, 2) : content.command : '';
  return <Modal title={site ? t('handoff.title', { site: site.title }) : t('handoff.titleGeneral')} subtitle={t('handoff.description')} onClose={onClose}><div className="filter-tabs handoff-tabs">{['config', 'command', 'instructions'].map(id => <button className={tab === id ? 'selected' : ''} key={id} onClick={() => setTab(id)}>{t(id === 'instructions' ? 'agents.instructions' : `handoff.${id}`)}</button>)}</div>{error ? <ErrorNotice code={error} /> : content ? <><pre className="handoff-content" tabIndex={0}>{value}</pre><p className="subtle-note">{t(tab === 'config' ? 'handoff.configHint' : tab === 'instructions' ? 'agents.instructionsHint' : 'handoff.localNote')}</p><Button onClick={() => { void copyText(value).then(() => ops.toast('common.copied')).catch(() => setError('internal')); }}><Copy size={15} />{t(tab === 'config' ? 'handoff.copyConfig' : tab === 'instructions' ? 'handoff.copyInstructions' : 'common.copy')}</Button></> : <p>{t('common.loading')}</p>}{site && <div className="handoff-actions"><h3>{t('handoff.available')}</h3>{site.actions.map(a => <span key={a.id}><Check size={13} />{a.title}</span>)}</div>}</Modal>;
}
// Workspace owns requests so closing a result dialog cannot forget an accepted
// operation or release an in-flight submission. Never persist result data to disk.
export function useSummaryRequests(jobs: Job[]) {
  const [requests, setRequests] = useState<Record<string, { pending: boolean; job?: Job }>>({});
  const submitting = useRef(new Set<string>());
  useEffect(() => {
    // An accepted response is only a bridge until that exact child is observed.
    // Thereafter the snapshot owns its status and any subsequent Activity retry.
    setRequests(current => {
      let next = current;
      for (const [parent, request] of Object.entries(current)) {
        if (!request.job || !jobs.some(job => job.id === request.job!.id && job.kind === 'summary' && job.parent_id === parent)) continue;
        if (next === current) next = { ...current };
        next[parent] = { pending: request.pending };
      }
      return next;
    });
  }, [jobs, requests]);
  return {
    requests,
    begin(parent: string) {
      if (submitting.current.has(parent)) return false;
      submitting.current.add(parent);
      setRequests(current => ({ ...current, [parent]: { ...current[parent], pending: true } }));
      return true;
    },
    accept(parent: string, job: Job) {
      setRequests(current => ({ ...current, [parent]: { ...current[parent], job } }));
    },
    finish(parent: string) {
      submitting.current.delete(parent);
      setRequests(current => ({ ...current, [parent]: { ...current[parent], pending: false } }));
    },
  };
}
export function ResultDialog({ job, data, ops, onClose, connectAccount, summaries }: { summaries: ReturnType<typeof useSummaryRequests>; job: Job; data: Snapshot; ops: Operations; onClose: () => void; connectAccount?: () => void }) {
  const { t, date } = useI18n(); const [error, setError] = useState(''); const working = ['running', 'queued'].includes(job.status);
  const [copyError, setCopyError] = useState('');
  const [exportError, setExportError] = useState('');
  const copying = useRef(false);
  const exporting = useRef(false);
  const [result, setResult] = useState<unknown>(job.result);
  const [artifact, setArtifact] = useState(job.artifact);
  const [resultLoading, setResultLoading] = useState(false);
  const file = job.kind === 'action' && job.status === 'completed' ? fileArtifact(artifact) : null;
  const summaryRequest = summaries.requests[job.id];
  const requestedSummary = summaryRequest?.job;
  const busy = !!summaryRequest?.pending;
  const summaryJobs = data.jobs.filter(child => child.kind === 'summary' && child.parent_id === job.id).sort((a, b) => Date.parse(b.started_at) - Date.parse(a.started_at));
  const summaryJob = requestedSummary?.parent_id === job.id && !summaryJobs.some(child => child.id === requestedSummary.id) ? requestedSummary : summaryJobs[0];
  const summaryPending = busy || summaryJobs.some(isWorking) || (!!summaryJob && isWorking(summaryJob));
  const summaryNeedsRetry = summaryJob && ['failed', 'cancelled'].includes(summaryJob.status);
  const summarize = async (retry?: Job) => {
    if (summaryPending || !summaries.begin(job.id)) return;
    setError('');
    try {
      const result = retry?.status === 'failed'
        ? await ops.act<{job: Job}>(`/api/jobs/${retry.id}/retry`, 'POST')
        : await ops.act<{job: Job}>(`/api/jobs/${job.id}/summarize`, 'POST', {provider: retry?.provider || data.settings.default_provider});
      summaries.accept(job.id, result.job);
    } catch (e) { setError(codeOf(e)); }
    finally { summaries.finish(job.id); }
  };

  useEffect(() => {
    let active = true;
    setResult(job.result);
    setArtifact(job.artifact);
    if (working || job.status !== 'completed') { setResultLoading(false); return; }
    setResultLoading(true);
    void request<Job>(`/api/jobs/${job.id}`).then(full => { if (active) { setResult(full.result); setArtifact(full.artifact); } }).catch(e => { if (active) setError(codeOf(e)); }).finally(() => { if (active) setResultLoading(false); });
    return () => { active = false; };
  }, [job.id, job.updated_at, job.status, working]);
  return <Modal title={job.action_title || job.site_title} subtitle={job.site_title} onClose={onClose} wide>{working ? <Progress job={job} onCancel={() => { void ops.act(`/api/jobs/${job.id}/cancel`).catch(e => setError(codeOf(e))); }} /> : <><div className="result-heading"><Badge tone={job.status === 'completed' ? 'green' : 'amber'}>{t(`activity.${job.status}`)}</Badge><span>{date(job.finished_at || job.updated_at)}</span></div>{job.error_code && <ErrorNotice code={job.error_code} />}{connectAccount && ['auth_required', 'blocked_by_site', 'browser_unavailable'].includes(job.error_code || '') && <Button variant="secondary" onClick={connectAccount}>{t('accounts.autoFind')}<ArrowRight size={15} /></Button>}{!file && job.summary && <div className="result-summary"><Sparkles size={18} /><div><h3>{t('actions.summary')}</h3><p>{job.summary}</p></div></div>}{resultLoading ? <p className="list-empty">{t('common.loading')}</p> : (file || result !== undefined) ? <>{file ? <FileResult key={`${job.id}:${file.artifact_id}`} file={file} job={job} onCopy={() => ops.toast('common.copied')} /> : <ResultValue value={result} />}<div className="result-actions"><Button variant="secondary" onClick={() => { if (copying.current) return; copying.current = true; setCopyError(''); void copyText(formatValue(file || result)).then(() => ops.toast('common.copied')).catch(() => setCopyError('internal')).finally(() => { copying.current = false; }); }}><Copy size={14} />{t(file ? 'files.copyManifest' : 'actions.copy')}</Button><Button variant="secondary" onClick={() => { if (exporting.current) return; exporting.current = true; setExportError(''); void download(`/api/jobs/${job.id}/result`, `hycli-${job.id.slice(0, 8)}.json`).catch(e => setExportError(codeOf(e))).finally(() => { exporting.current = false; }); }}><ArrowDownToLine size={14} />{t(file ? 'files.exportManifest' : 'actions.download')}</Button>{!file && !summaryNeedsRetry && <Button disabled={job.status !== 'completed' || !data.providers.some(p => p.connected) || summaryPending} loading={busy} onClick={() => { void summarize(); }}><Sparkles size={14} />{t('actions.summary')}</Button>}</div></> : job.status === 'completed' && !job.error_code && !error ? <p className="list-empty">{t('actions.noResult')}</p> : null}</>}{!working && (job.notes?.length || 0) > 0 && <section className="work-log"><h3>{t('activity.workLog')}</h3><WorkNotes job={job} /></section>}{!file && summaryJob && summaryJob.status !== 'completed' && <section className="summary-operation" aria-label={t('actions.summary')} aria-live="polite"><h3>{t('actions.summary')}</h3>{isWorking(summaryJob) ? <Progress job={summaryJob} compact /> : summaryNeedsRetry ? <><Badge tone="amber">{t(`activity.${summaryJob.status}`)}</Badge><p>{t('errors.no_summary')}</p>{summaryJob.error_code && summaryJob.error_code !== 'no_summary' && <ErrorNotice code={summaryJob.error_code} />}<Button variant="secondary" loading={busy} disabled={summaryPending || !data.providers.some(p => p.connected)} onClick={() => { void summarize(summaryJob); }}>{t('common.retry')}</Button></> : null}</section>}{copyError && <ErrorNotice code={copyError} />}{exportError && <ErrorNotice code={exportError} />}{error && <ErrorNotice code={error} />}</Modal>;
}
function ResultValue({ value, depth = 0 }: { value: unknown; depth?: number }) {
  const { t, number } = useI18n();
  const scalar = (v: unknown) => v === null || v === undefined ? '—' : typeof v === 'boolean' ? t(v ? 'common.yes' : 'common.no') : typeof v === 'number' ? number(v) : String(v);
  if (depth > 5) return <pre className="result-text">{formatValue(value)}</pre>;
  if (Array.isArray(value) && value.length && value.every(v => v && typeof v === 'object' && !Array.isArray(v))) {
    const columns = [...new Set(value.flatMap(v => Object.keys(v)))].filter(c => value.some(v => v[c] !== '[private]')).slice(0, 12);
    return <div className="result-table-wrap"><table className="result-table"><thead><tr>{columns.map(c => <th key={c}>{c.replaceAll('_', ' ')}</th>)}</tr></thead><tbody>{value.slice(0, 500).map((v, i) => <tr key={i}>{columns.map(c => <td key={c}>{v[c] && typeof v[c] === 'object' ? <ResultValue value={v[c]} depth={depth + 1} /> : scalar(v[c])}</td>)}</tr>)}</tbody></table></div>;
  }
  if (value && typeof value === 'object' && !Array.isArray(value)) {
    const entries = Object.entries(value).filter(([, v]) => v !== '[private]');
    if (entries.length === 1 && entries[0][1] && typeof entries[0][1] === 'object') return <ResultValue value={entries[0][1]} depth={depth + 1} />;
    return <dl className="result-fields">{entries.map(([key, v]) => <div key={key} className={v && typeof v === 'object' ? 'result-field--nested' : ''}><dt>{key.replaceAll('_', ' ')}</dt><dd>{v && typeof v === 'object' ? <ResultValue value={v} depth={depth + 1} /> : scalar(v)}</dd></div>)}</dl>;
  }
  if (Array.isArray(value)) return <ul className="result-list">{value.slice(0, 500).map((v, i) => <li key={i}><ResultValue value={v} depth={depth + 1} /></li>)}</ul>;
  return <span className="result-scalar">{scalar(value)}</span>;
}
