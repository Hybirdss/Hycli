import { useEffect, useRef, useState, type ReactNode, type ButtonHTMLAttributes } from 'react';
import { ArrowRight, Check, ChevronDown, LoaderCircle, X, Globe2, Search, AlertCircle } from 'lucide-react';
import { createPortal } from 'react-dom';
import { useI18n } from './i18n';

export function Bird({ mood = 'idle', className = '' }: { mood?: string; className?: string }) {
  return <img src={mood === 'logo' ? '/brand/shima.png' : '/brand/shima-idle-v2.png'} alt="" aria-hidden="true" className={`bird bird--${mood} ${className}`} draggable="false" />;
}
export function Wordmark({ small = false }: { small?: boolean }) {
  return <span className={`wordmark ${small ? 'wordmark--small' : ''}`}><Bird mood="logo" /><span dir="ltr">Hycl<span className="wordmark-i">ı<span /></span></span></span>;
}
export function Button({ children, loading, variant = 'primary', className = '', ...props }: ButtonHTMLAttributes<HTMLButtonElement> & { loading?: boolean; variant?: 'primary' | 'secondary' | 'ghost' | 'danger' }) {
  return <button className={`button button--${variant} ${className}`} {...props} disabled={props.disabled || loading}>{loading && <LoaderCircle size={16} className="spin" />}{children}</button>;
}
export function IconButton({ children, label, className = '', ...props }: ButtonHTMLAttributes<HTMLButtonElement> & { label: string }) { return <button className={`icon-button ${className}`} aria-label={label} title={label} {...props}>{children}</button>; }
export function Badge({ children, tone = 'neutral' }: { children: ReactNode; tone?: 'neutral' | 'green' | 'amber' | 'red' }) { return <span className={`badge badge--${tone}`}>{tone === 'green' && <span className="status-dot" />}{children}</span>; }
export function SearchField({ value, onChange, placeholder }: { value: string; onChange: (v: string) => void; placeholder: string }) {
  const { t } = useI18n();
  return <label className="search-field"><Search size={17} /><input value={value} onChange={e => onChange(e.target.value)} placeholder={placeholder} aria-label={placeholder} />{value && <button onClick={() => onChange('')} aria-label={t('common.clear')}><X size={14} /></button>}</label>;
}
export function SiteIcon({ title, image, size = 'normal' }: { title: string; image?: string; size?: 'normal' | 'large' }) {
  const [failed, setFailed] = useState(false);
  useEffect(() => setFailed(false), [image]);
  const color = [...title].reduce((n, c) => n + c.charCodeAt(0), 0) % 5;
  return <span className={`site-icon site-icon--${color} site-icon--${size} ${image && !failed ? 'site-icon--image' : ''}`} aria-hidden="true">{image && !failed ? <img src={image} alt="" onError={() => setFailed(true)} /> : title.trim() ? [...title.trim()][0].toLocaleUpperCase() : <Globe2 size={24} />}</span>;
}
export function ProviderMark({ id }: { id: string }) {
  const family = ['codex', 'openai', 'chatgpt'].includes(id) ? 'chatgpt' : id.startsWith('zai') ? 'zai' : id;
  return <span className={`provider-mark provider-mark--${family}`} aria-hidden="true">{family === 'chatgpt' ? <><img className="brand-light" src="/brand/providers/chatgpt-black.svg" alt="" /><img className="brand-dark" src="/brand/providers/chatgpt-white.svg" alt="" /></> : <img src={`/brand/providers/${family === 'anthropic' ? 'claude.png' : family + '.svg'}`} alt="" />}</span>;
}
export function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) { return <label className="field"><span>{label}</span>{children}{hint && <small>{hint}</small>}</label>; }
export function Select({ children, ...props }: React.SelectHTMLAttributes<HTMLSelectElement>) { return <span className="select-wrap"><select {...props}>{children}</select><ChevronDown size={14} /></span>; }
export function ErrorNotice({ code, onRetry }: { code: string; onRetry?: () => void }) { const { t } = useI18n();return <div className="notice notice--error" role="alert"><AlertCircle size={18} /><span>{t(`errors.${code}`)}</span>{onRetry && <button onClick={onRetry}>{t('common.retry')}</button>}</div>; }
export function EmptyState({ title, description, children, compact = false }: { title: string; description: string; children?: ReactNode; compact?: boolean }) { return <div className={`empty-state ${compact ? 'empty-state--compact' : ''}`}><div className="empty-bird"><Bird /></div><h2>{title}</h2><p>{description}</p>{children}</div>; }

export function Modal({ title, subtitle, children, onClose, wide = false }: { title: string; subtitle?: string; children: ReactNode; onClose: () => void; wide?: boolean }) {
  const { t } = useI18n(); const dialog = useRef<HTMLDialogElement>(null);const close = useRef(onClose);close.current = onClose;
  useEffect(() => { const element = dialog.current!; element.showModal(); const saved = document.body.style.overflow; document.body.style.overflow = 'hidden'; return () => { element.close(); document.body.style.overflow = saved; }; }, []);
  return createPortal(<dialog ref={dialog} className={`modal ${wide ? 'modal--wide' : ''}`} onCancel={e => { e.preventDefault(); close.current(); }} onClick={e => { if (e.target === dialog.current) { const r = dialog.current!.getBoundingClientRect(); if (e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom) onClose(); } }} aria-labelledby="dialog-title"><div className="modal-heading"><div><h2 id="dialog-title">{title}</h2>{subtitle && <p>{subtitle}</p>}</div><IconButton label={t('common.close')} onClick={onClose}><X size={20} /></IconButton></div><div className="modal-content">{children}</div></dialog>, document.body);
}
export function ArrowLink({ children, onClick }: { children: ReactNode; onClick: () => void }) { return <button className="arrow-link" onClick={onClick}>{children}<ArrowRight size={15} /></button>; }
export function Toggle({ label, description, value, onChange }: { label: string; description: string; value: boolean; onChange: (v: boolean) => void }) { return <div className="setting-row"><div><h3>{label}</h3><p>{description}</p></div><button role="switch" aria-checked={value} aria-label={label} className={`toggle ${value ? 'toggle--on' : ''}`} onClick={() => onChange(!value)}><span /></button></div>; }
