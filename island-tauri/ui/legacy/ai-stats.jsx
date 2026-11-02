import React, { useEffect, useState } from "react";

const number = (value) => Number.isFinite(value) ? value.toLocaleString() : "Unavailable";
const money = (value) => Number.isFinite(value) ? new Intl.NumberFormat(undefined, { style: "currency", currency: "USD", maximumFractionDigits: 3 }).format(value) : "Unavailable";
const date = (value) => value && Number.isFinite(Date.parse(value)) ? new Date(value).toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" }) : "Unavailable";

export default function AIStats() {
  const [data, setData] = useState(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [provider, setProvider] = useState("openai-codex");
  const [connecting, setConnecting] = useState(false);
  const refresh = async () => {
    setBusy(true); setError("");
    try {
      if (!window.island.aiUsage) throw new Error("Restart Ripple to enable AI usage integration.");
      const next = await window.island.aiUsage();
      setData(next);
    } catch { setError("Could not load AI usage. Check Hermes account access and try again."); }
    finally { setBusy(false); }
  };
  useEffect(() => {
    let live = true;
    if (!window.island.aiUsage) { setError("Restart Ripple to enable AI usage integration."); return; }
    setBusy(true);
    window.island.aiUsage().then(next => { if (live) { setData(next); setProvider(next.account?.provider || "openai-codex"); } })
      .catch(() => { if (live) setError("Could not load AI usage. Check Hermes account access and try again."); })
      .finally(() => { if (live) setBusy(false); });
    return () => { live = false; };
  }, []);
  const connect = async () => {
    setConnecting(true); setNotice("");
    try {
      const result = await window.island.aiConnect(provider);
      if (!result?.launched) throw new Error("Sign-in was not launched");
      setNotice("Complete sign-in in the secure Hermes terminal, then refresh. Account limits follow your active Hermes provider.");
    } catch { setNotice("Could not open sign-in. Use hermes auth add in a terminal."); }
    finally { setConnecting(false); }
  };
  const account = data?.account;
  const local = data?.local;
  const totals = local?.totals;
  return <div className="settings-panel ai-stats" aria-label="AI usage statistics" aria-busy={busy}>
    <div className="ai-heading"><div><h3>AI stats</h3><p className="settings-note">Account allowance and local activity — kept separate.</p></div><button className="restart" disabled={busy} onClick={refresh}>{busy ? "Loading…" : "Refresh"}</button></div>
    {error && <p role="alert">{error}</p>}
    <section className="ai-card">
      <div className="ai-heading"><strong>{account?.provider || "Hermes account"}</strong>{account?.plan && <span className="ai-plan">{account.plan}</span>}</div>
      {(account?.windows || []).map((window, index) => {
        const used = Number.isFinite(window.used_percent) ? Math.max(0, Math.min(100, window.used_percent)) : null;
        return <div className="ai-window" key={`${window.label}-${index}`}>
          <div className="gauge-head"><span>{window.label}</span><strong>{used === null ? "Unavailable" : `${Number((100 - used).toFixed(1))}% remaining`}</strong></div>
          <div className="gauge-track" role={used === null ? undefined : "progressbar"} aria-label={`${window.label} usage`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={used ?? undefined}><div className="gauge-fill" style={{width: `${used || 0}%`}} /></div>
          <p className="settings-note">{used === null ? "Usage unavailable" : `${used}% used`} · Resets {date(window.resets_at)}</p>
          {window.detail && <p className="settings-note">{window.detail}</p>}
        </div>;
      })}
      {account?.unavailable_reason && <p className="settings-note">{account.unavailable_reason}</p>}
      {(account?.details || []).map((detail, index) => <p className="settings-note" key={index}>{detail}</p>)}
      <p className="settings-note">Provider-reported limits, not a conversion to tokens. API billing and chat subscriptions have separate allowances.</p>
    </section>
    <section className="ai-card">
      <h4>Local Hermes activity · sessions started in the last 30 days</h4>
      {!local || local.available === false ? <p className="settings-note">{local?.unavailableReason || "Local accounting unavailable"}</p> : <>
        <div className="ai-metrics"><div><span>Input tokens</span><strong>{number(totals?.inputTokens)}</strong></div><div><span>Output tokens</span><strong>{number(totals?.outputTokens)}</strong></div><div><span>Sessions</span><strong>{number(totals?.sessions)}</strong></div><div><span>Estimated API cost</span><strong>{money(totals?.estimatedCostUsd)}</strong></div></div>
        <p className="settings-note">Local recorded usage for this session cohort, not a per-call date range or your account-wide total. Estimated API-equivalent cost is not a subscription charge.</p>
      </>}
    </section>
    <section className="ai-card">
      <h4>Connect a provider</h4>
      <div className="ai-connect"><select aria-label="AI provider" value={provider} onChange={event => setProvider(event.target.value)}>{(data?.providers || [{id:"openai-codex",label:"ChatGPT / Codex"},{id:"anthropic",label:"Anthropic"},{id:"openrouter",label:"OpenRouter"},{id:"nous",label:"Nous"}]).map(item => <option key={item.id} value={item.id}>{item.label}</option>)}</select><button className="restart" disabled={connecting} onClick={connect}>Sign in</button></div>
      <p className="settings-note">Uses Hermes-managed authentication in a separate terminal. Ripple never receives your password or API key. Adding credentials does not change your active model/provider.</p>
      {notice && <p className="settings-note" role="status">{notice}</p>}
    </section>
    {data?.fetchedAt && <p className="settings-note">Checked {date(data.fetchedAt)} · account data is cached briefly to avoid excessive requests.</p>}
  </div>;
}
