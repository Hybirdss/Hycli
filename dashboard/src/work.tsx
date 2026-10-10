import { useEffect, useState, type CSSProperties } from 'react';
import { ArrowRight, Check, Clock3, Sparkles, X } from 'lucide-react';
import { Badge, Button } from './ui';
import { useI18n } from './i18n';
import type { Job, JobNote } from './types';

export const isWorking = (job: Job) => ['running', 'queued'].includes(job.status);
export function MotionBird({ motion = 'waiting', className = '' }: { motion?: 'scurry' | 'waiting' | 'celebrate'; className?: string }) {
  return <span className={`motion-bird motion-bird--${motion} ${className}`} aria-hidden="true" />;
}
export function providerName(id: string, name: string, t: (key: string) => string) { return id === 'codex' ? t('connections.loginShort') : id === 'openai' ? t('connections.apiShort') : name; }
function Elapsed({ job }: { job: Job }) {
  const { t, number } = useI18n(); const [now, setNow] = useState(Date.now());
  useEffect(() => { if (!isWorking(job)) return; const timer = window.setInterval(() => setNow(Date.now()), 1000); return () => clearInterval(timer); }, [job.status]);
  const seconds = Math.max(0, Math.floor(((job.finished_at ? new Date(job.finished_at).getTime() : now) - new Date(job.started_at).getTime()) / 1000));
  const time = `${number(Math.floor(seconds / 60))}:${String(seconds % 60).padStart(2, '0')}`;
  return <span className="work-elapsed"><Clock3 size={13} />{t('activity.elapsed', { time })}</span>;
}
export function WorkNotes({ job, limit }: { job: Job; limit?: number }) {
  const { t, locale } = useI18n(); const notes = job.notes || []; const shown = limit ? notes.slice(-limit) : notes;
  const text = (note: JobNote) => note.code === 'agent_summary' ? note.message : t(`activity.note.${note.code}`, { count: note.count });
  return <ol className="work-notes">{shown.length ? shown.map(note => <li key={note.seq}><span className={`note-dot ${note.code === 'completed' ? 'note-dot--done' : ''}`} /><div><p>{text(note)}</p><span>{note.agent && <strong>{t(`activity.role.${note.agent}`)} · </strong>}<time dateTime={note.at}>{new Intl.DateTimeFormat(locale, { hour: '2-digit', minute: '2-digit', second: '2-digit' }).format(new Date(note.at))}</time></span></div></li>) : <li className="notes-empty">{t('activity.noNotes')}</li>}</ol>;
}
export function Progress({ job, onCancel, onOpen, compact = false }: { job: Job; onCancel?: () => void; onOpen?: () => void; compact?: boolean }) {
  const { t, number } = useI18n(); const active = isWorking(job); const total = job.progress_total || (job.kind === 'prepare' ? 5 : 2); const done = Math.min(total, job.status === 'completed' ? total : job.progress_done || 0); const progress = done / total * 100;
  const [tick, setTick] = useState(0);
  useEffect(() => { if (!active) return; const timer = window.setInterval(() => setTick(x => x + 1), 7000); return () => clearInterval(timer); }, [active]);
  const motion = job.status === 'completed' ? 'celebrate' : tick % 3 === 0 && job.agents?.some(a => a.status === 'running') ? 'scurry' : 'waiting';
  return <section className={`work-panel ${compact ? 'work-panel--compact' : ''} ${active ? 'work-panel--active' : ''}`} aria-label={job.site_title}>
    <div className="work-main"><div className="work-heading"><div><span className="eyebrow"><span className={`status-dot ${active ? 'status-dot--live' : ''}`} />{t(active ? 'activity.inProgress' : `activity.${job.status}`)}</span><h2>{job.action_title || job.site_title || t('activity.preparingTitle')}</h2><p aria-live="polite">{t(`activity.phase.${job.phase}`)}</p></div>{onCancel && active && <button className="work-stop" onClick={onCancel} aria-label={t('activity.cancel')}><X size={14} /><span>{t('activity.cancel')}</span></button>}</div>
    <div className="work-track-space"><div className="work-track" role="progressbar" aria-label={t('activity.status')} aria-valuemin={0} aria-valuemax={total} aria-valuenow={done} aria-valuetext={t('activity.stages', { done, total })}><span className="work-track-fill" style={{ width: `${progress}%` }} /><span className="work-bird-position" style={{ '--progress': `${Math.max(5, Math.min(94, progress))}%` } as CSSProperties}><MotionBird motion={motion} /></span></div></div>
    {job.understanding && <section className="workflow-understanding"><p>{job.understanding.summary}</p><ul>{job.understanding.workflows.map(workflow => <li key={workflow.id}>{workflow.title}</li>)}</ul></section>}
    <div className="work-meta"><span>{t('activity.stages', { done: number(done), total: number(total) })}</span><Elapsed job={job} /></div>
    {job.kind === 'prepare' && <><ol className="work-stages">{['understand', 'read', 'prepare', 'verify', 'ready'].map((phase, i) => <li key={phase} className={i < done ? 'done' : i === done ? 'current' : ''}><span>{i < done ? <Check size={11} /> : i + 1}</span>{t(`activity.stage.${phase}`)}</li>)}</ol><div className="work-agents">{['explore', 'organize', 'check'].map(role => { const agent = job.agents?.find(a => a.id === role); return <div key={role} className={`work-agent work-agent--${agent?.status || 'idle'}`}><div><span className="agent-emblem"><Sparkles size={15} /></span><strong>{t(`activity.role.${role}`)}</strong><span className="agent-indicator" /></div><p>{t(`activity.roleHint.${role}`)}</p><span>{agent?.status === 'completed' ? t('activity.actionsFound', { count: number(agent.found) }) : t(`activity.agent.${agent?.status || 'idle'}`)}</span></div>; })}</div></>}
    </div><aside className="work-live"><div className="work-live-title"><h3>{t('activity.liveNotes')}</h3>{active && <Badge tone="green">{t('activity.agent.running')}</Badge>}</div><WorkNotes job={job} limit={compact ? 3 : 4} />{onOpen && <Button variant="ghost" onClick={onOpen}>{t('activity.viewWork')}<ArrowRight size={14} /></Button>}</aside>
  </section>;
}
export function RunningIndicator({ jobs, onClick }: { jobs: Job[]; onClick: () => void }) {
  const { t } = useI18n(); const running = jobs.filter(isWorking); const agents = running.flatMap(j => j.agents || []).filter(a => a.status === 'running').length;
  if (!running.length) return null;
  return <button className="running-indicator" onClick={onClick}><span className="status-dot status-dot--live" /><MotionBird motion="scurry" /><span>{t(agents ? 'activity.agentsCount' : 'activity.workingCount', { count: agents || running.length })}</span></button>;
}
