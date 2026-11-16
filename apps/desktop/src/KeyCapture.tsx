import { useEffect, useRef, useState } from 'react';
import { backend, type CaptureStatus } from './lib/backend';
import type { getTranslation } from './i18n/translations';

type T = ReturnType<typeof getTranslation>;

export function formatCountdown(seconds: number): string {
  const safe = Math.max(0, Math.floor(seconds));
  return `${Math.floor(safe / 60)}:${String(safe % 60).padStart(2, '0')}`;
}

/** Status line for a finished capture; `validation` is the provider check result of the new key. */
export function captureMessage(t: T, status: CaptureStatus): string {
  if (status.state === 'timeout') return t.capture.timeout;
  if (status.state === 'cancelled') return t.capture.cancelled;
  if (status.state === 'captured') {
    if (!status.validation) return t.capture.checking;
    if (status.validation === 'valid') return t.capture.valid;
    if (status.validation === 'unauthorized' || status.validation === 'invalid') return t.capture.badKey;
    return `${t.capture.checked}: ${status.validation}`;
  }
  return t.capture.waiting;
}

/**
 * Countdown dialog while Desktop watches the clipboard. It only ever shows state, the
 * remaining time and a secret id; the captured key never reaches the UI.
 */
export function CaptureModal({ t, providerName, onCaptured, onClose }: { t: T; providerName: string; onCaptured: (secretId: string) => void; onClose: () => void }) {
  const [status, setStatus] = useState<CaptureStatus | null>(null);
  const notified = useRef(false);
  useEffect(() => {
    let stopped = false;
    const poll = () => { backend.captureStatus().then((next) => { if (!stopped) setStatus(next); }).catch(() => undefined); };
    poll();
    const timer = window.setInterval(poll, 500);
    return () => { stopped = true; window.clearInterval(timer); };
  }, []);
  useEffect(() => {
    if (status?.state === 'captured' && status.secretId && !notified.current) { notified.current = true; onCaptured(status.secretId); }
  }, [status, onCaptured]);

  const waiting = !status || status.state === 'waiting' || status.state === 'idle';
  async function cancel() { try { setStatus(await backend.captureCancel()); } catch { onClose(); } }
  return <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={t.capture.title}>
    <div className="secret-form capture-modal">
      <div className="form-heading"><div><span className="section-kicker">CLIPBOARD CAPTURE / {providerName.toUpperCase()}</span><h2>{t.capture.title}</h2></div></div>
      <p className="form-intro">{t.capture.privacy}</p>
      <div className="capture-status" role="status">{status ? captureMessage(t, status) : t.capture.waiting}</div>
      {waiting && status && <div className="capture-countdown" aria-label={t.capture.countdownLabel}>{formatCountdown(status.remainingSeconds)}</div>}
      <div className="form-actions">
        {waiting ? <button type="button" className="secondary-button" onClick={() => void cancel()}>{t.capture.cancel}</button> : <button type="button" className="primary-button" onClick={onClose}>{t.capture.done}</button>}
      </div>
    </div>
  </div>;
}
