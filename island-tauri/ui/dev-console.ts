// Dev-only: forward page errors to the local console sink (WebKitGTK has no CDP).
// Imported first by island-main so it is evaluated before every other module.
if (import.meta.env.DEV) {
  const report = (kind: string, detail: unknown) => {
    const text = detail instanceof Error ? `${detail.message}\n${detail.stack ?? ''}` : String(detail);
    const body = JSON.stringify({ kind, text });
    void fetch('http://127.0.0.1:5199/', { method: 'POST', body }).catch(() => { new Image().src = `http://127.0.0.1:5199/?${encodeURIComponent(body.slice(0, 1800))}`; });
  };
  addEventListener('error', e => report('error', e.error ?? e.message));
  addEventListener('unhandledrejection', e => report('rejection', e.reason));
  const original = console.error.bind(console);
  console.error = (...args: unknown[]) => { report('console.error', args.map(a => a instanceof Error ? `${a.message}\n${a.stack}` : String(a)).join(' ')); original(...args); };
  report('boot', location.href);
  // Dev-only remote: the controller can queue a JS snippet (navigation/inspection) to run here.
  setInterval(() => {
    void fetch('http://127.0.0.1:5199/cmd').then(r => r.text()).then(async code => {
      if (!code) return;
      try { report('result', JSON.stringify(await new Function(`return (async () => { ${code} })()`)())); }
      catch (e) { report('cmd-error', e); }
    }).catch(() => undefined);
  }, 700);
}
export {};
