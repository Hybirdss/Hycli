import { useEffect, useRef, useState } from 'react';
import { ArrowDownToLine, Copy, FileCheck2 } from 'lucide-react';
import { ApiError, checkArtifact, copyText, downloadArtifact } from './api';
import { useI18n } from './i18n';
import { Button, ErrorNotice } from './ui';

import type { FileArtifact, Job } from './types';

export function fileArtifact(value: unknown): FileArtifact | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const file = value as Partial<FileArtifact>;
  if (file.kind !== 'file' || file.version !== 1 || file.status !== 'complete'
    || file.location_scope !== 'hycli_host' || file.content_trust !== 'untrusted'
    || typeof file.artifact_id !== 'string' || !file.artifact_id
    || typeof file.filename !== 'string' || typeof file.local_path !== 'string'
    || typeof file.media_type !== 'string' || typeof file.sha256 !== 'string'
    || typeof file.created_at !== 'string' || !Number.isFinite(Date.parse(file.created_at))
    || !file.provenance || typeof file.provenance.site !== 'string'
    || typeof file.provenance.action !== 'string' || typeof file.provenance.source_origin !== 'string'
    || !/^[a-f0-9]{64}$/.test(file.sha256)
    || !Number.isSafeInteger(file.byte_count) || (file.byte_count ?? -1) < 0) return null;
  return file as FileArtifact;
}

type FileAccess = { status: 'notChecked' | 'checking' } | { status: 'available'; requested: boolean } | { status: 'unavailable' | 'checkFailed'; error: string };

export function completedFiles(jobs: Job[]): { job: Job; file: FileArtifact }[] {
  return jobs.flatMap(job => {
    const file = job.kind === 'action' && job.status === 'completed' ? fileArtifact(job.artifact) : null;
    return file ? [{ job, file }] : [];
  });
}

export function FileHistory({ jobs, onCopy }: { jobs: Job[]; onCopy: () => void }) {
  const { t } = useI18n();
  const files = completedFiles(jobs);
  return <section className="file-history" aria-label={t('files.listTitle')}>
    <h2>{t('files.listTitle')}</h2><p className="subtle-note">{t('files.listScope')}</p>
    {!files.length ? <p className="list-empty">{t('files.listEmpty')}</p> : <ul className="file-history-list">{files.map(({ job, file }) => <li key={`${job.id}:${file.artifact_id}`}>
      <FileResult file={file} job={job} onCopy={onCopy} />
    </li>)}</ul>}
  </section>;
}

export function FileResult({ file, job, onCopy }: {
  file: FileArtifact; job: Job; onCopy: () => void;
}) {
  const { t, number, date } = useI18n();
  const active = useRef<AbortController | null>(null);
  const [access, setAccess] = useState<FileAccess>({ status: 'notChecked' });
  const [copyError, setCopyError] = useState('');
  useEffect(() => () => { active.current?.abort(); active.current = null; }, []);
  const retrieve = async (save: boolean) => {
    if (active.current) return;
    const controller = new AbortController(); active.current = controller;
    setAccess({ status: 'checking' });
    try {
      if (save) await downloadArtifact(job.id, file.artifact_id, file.filename, controller.signal);
      else await checkArtifact(job.id, file.artifact_id, controller.signal);
      if (!controller.signal.aborted) setAccess({ status: 'available', requested: save });
    } catch (error) {
      if (!controller.signal.aborted) setAccess({ status: error instanceof ApiError && [400, 404, 409].includes(error.status) ? 'unavailable' : 'checkFailed', error: error instanceof ApiError ? error.code : 'internal' });
    } finally { if (active.current === controller) active.current = null; }
  };
  const busy = access.status === 'checking';
  return <section className="file-result" aria-label={t('files.ready')} data-job-id={job.id} data-artifact-id={file.artifact_id}>
    <div className="file-result-heading"><FileCheck2 size={24} /><div><h3>{file.filename}</h3><p>{file.media_type} · {t('files.bytes', { count: number(file.byte_count) })}</p></div></div>
    <dl className="file-metadata">
      <div><dt>{t('files.purpose')}</dt><dd>{job.action_title || file.provenance.action}</dd></div>
      <div><dt>{t('files.source')}</dt><dd>{file.provenance.source_origin} · {file.provenance.site} / {file.provenance.action}</dd></div>
      <div><dt>{t('files.artifactId')}</dt><dd><code>{file.artifact_id}</code></dd></div>
      <div><dt>{t('files.jobId')}</dt><dd><code>{job.id}</code></dd></div>
      <div><dt>{t('files.created')}</dt><dd><time dateTime={file.created_at}>{date(file.created_at)}</time></dd></div>
    </dl>
    <p>{t('files.localOnly')}</p>
    <div className="file-result-path"><code>{file.local_path}</code><Button variant="secondary" onClick={() => { setCopyError(''); void copyText(file.local_path).then(onCopy).catch(() => setCopyError('internal')); }}><Copy size={14} />{t('files.copyPath')}</Button></div>
    <dl className="file-states" aria-live="polite">
      <div><dt>{t('files.completion')}</dt><dd>{t('activity.completed')}</dd></div>
      <div><dt>{t('files.availability')}</dt><dd>{t(busy ? 'common.loading' : `files.${access.status}`)}</dd></div>
      <div><dt>{t('files.inspection')}</dt><dd>{t('files.notInspected')}</dd></div>
    </dl>
    <details><summary>{t('files.integrity')}</summary><code className="file-result-hash">SHA-256 {file.sha256}</code></details>
    <p className="subtle-note">{t('files.untrusted')}</p>
    <div className="file-result-actions"><Button loading={busy} onClick={() => { void retrieve(true); }}><ArrowDownToLine size={15} />{t('files.download')}</Button><Button variant="secondary" disabled={busy} onClick={() => { void retrieve(false); }}>{t('files.check')}</Button></div>
    {access.status === 'available' && access.requested && <p className="file-download-status" role="status">{t('files.started')}</p>}
    {'error' in access && <ErrorNotice code={access.error} />}
    {copyError && <ErrorNotice code={copyError} />}
  </section>;
}
