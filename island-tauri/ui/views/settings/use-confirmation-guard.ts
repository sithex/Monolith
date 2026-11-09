import { useEffect, useRef, useState } from 'react';
import { CONFIRM_ARM_MS, CONFIRM_EXPIRE_MS, confirmationPhase } from './confirmation-guard';

export function useConfirmationGuard<T>() {
  const [request, setRequest] = useState<{ value: T; startedAt: number } | null>(null);
  const current = useRef(request);
  const [ready, setReady] = useState(false);
  const disarm = () => { current.current = null; setRequest(null); setReady(false); };
  const arm = (value: T) => {
    const next = { value, startedAt: performance.now() };
    current.current = next; setReady(false); setRequest(next);
  };
  useEffect(() => {
    if (!request) return;
    const elapsed = performance.now() - request.startedAt;
    const armTimer = setTimeout(() => setReady(true), Math.max(0, CONFIRM_ARM_MS - elapsed));
    const expireTimer = setTimeout(disarm, Math.max(0, CONFIRM_EXPIRE_MS - elapsed));
    return () => { clearTimeout(armTimer); clearTimeout(expireTimer); };
  }, [request]);
  const accept = () => {
    const phase = confirmationPhase(current.current?.startedAt ?? null, performance.now());
    if (phase !== 'ready') { if (phase === 'idle') disarm(); return false; }
    // Consume synchronously: a repeated event cannot submit the same confirmation twice.
    disarm(); return true;
  };
  return { pending: request?.value ?? null, ready, arm, disarm, accept };
}
