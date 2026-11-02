import React, { useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { motion, AnimatePresence, useReducedMotion } from "framer-motion";
import { Terminal } from "@xterm/xterm/lib/xterm.js";
import { FitAddon } from "@xterm/addon-fit";
import { dispatch as dispatchIntent, subscribeCue, useIslandState } from "../ipc/island-state";
import { toLegacyMode } from "../ipc/legacy-mode";
import paletteTools from "./palette.cjs";
import clickSoundTools from "./click-sound.cjs";
import AIStats from "./ai-stats.jsx";
import Monitor from "./monitor.jsx";
import "@xterm/xterm/css/xterm.css";
import "./style.css";
const api = window.island;
const playClick = clickSoundTools.createClickSound();
const UI_SPRING = { type: "spring", stiffness: 350, damping: 32, mass: 0.8 };
function Control(props) {
  const reduced = useReducedMotion();
  return <motion.button type="button" data-ui-control="true" whileHover={{ scale: reduced ? 1 : 1.06 }}
    whileTap={{ scale: reduced ? 1 : 0.94 }} transition={UI_SPRING} {...props} />;
}

const VIEWS = [
  { id: "terminal", label: "Terminal" },
  { id: "monitor", label: "Monitor" },
  { id: "ai", label: "AI stats" },
  { id: "settings", label: "Settings" },
];

const SETTINGS_DEFAULTS = {
  autostart: false,
  showInTaskbar: false,
  stealth: false,
  clickSounds: false,
  hoverDelay: 180,
  offsetX: 0,
  offsetY: 0,
  accent: "",
  background: "",
  borderEnabled: true,
  borderColor: "",
  panelClockColor: "",
  uiFont: "system",
  showDate: true,
  hardwareMonitor: true,
  terminalFontSize: 13,
};

const ACCENT_SWATCHES = ["", "#dddddd", "#f2b21a", "#4f8bff", "#ff5d5d", "#3fd68b", "#c27bff"];
const BG_SWATCHES = ["", "#111111", "#000000", "#1b1e28", "#232323", "#10243a", "#101414"];

function Toggle({ on, onChange, label, hint }) {
  return (
    <div className="setting-row">
      <div className="setting-text">
        <span className="setting-label">{label}</span>
        {hint && <span className="setting-hint">{hint}</span>}
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={on}
        className={`toggle ${on ? "on" : ""}`}
        onClick={() => onChange(!on)}
        aria-label={label}
      >
        <span className="knob" />
      </button>
    </div>
  );
}

function Slider({ value, min, max, step = 1, onChange, label, unit = "" }) {
  return (
    <div className="setting-row">
      <div className="setting-text"><span className="setting-label">{label}</span></div>
      <div className="slider-wrap">
        <input
          type="range"
          min={min}
          max={max}
          step={step}
          value={value}
          onChange={(e) => onChange(Number(e.target.value))}
          aria-label={label}
        />
        <span className="slider-value">{value}{unit}</span>
      </div>
    </div>
  );
}

function ColorPicker({ value, onChange, label, swatches }) {
  const [draft, setDraft] = useState(value);
  const [invalid, setInvalid] = useState(false);
  useEffect(() => { setDraft(value); setInvalid(false); }, [value]);
  const commit = () => {
    const text = draft.trim();
    if (text && !/^#?[0-9a-f]{6}$/i.test(text)) { setInvalid(true); return; }
    const hex = text ? `#${text.replace(/^#/, "").toLowerCase()}` : "";
    setInvalid(false);
    setDraft(hex);
    if (hex !== value) onChange(hex);
  };
  return (
    <div className="setting-row">
      <div className="setting-text">
        <span className="setting-label">{label}</span>
        <span className="setting-hint">blank = follow Hermes skin</span>
      </div>
      <div className="color-controls">
      <div className="swatches">
        {swatches.map((hex) => (
          <button
            key={hex || "skin"}
            type="button"
            className={`swatch ${value === hex ? "active" : ""}`}
            style={hex ? { background: hex } : { background: "linear-gradient(135deg,#333,#111)" }}
            title={hex || "Follow skin"}
            aria-label={hex || "Follow skin"}
            onClick={() => onChange(hex)}
          />
        ))}
        <label className="swatch custom" title="Custom color" style={{ "--chosen-color": value || "var(--skin-accent)" }}>
          <span className="custom-caption" aria-hidden="true">Custom</span>
          <input
            type="color"
            value={value || "#dddddd"}
            onChange={(e) => onChange(e.target.value)}
            aria-label={`${label} custom`}
          />
        </label>
      </div>
      <input className="color-hex" type="text" spellCheck={false} placeholder="Follow skin"
        aria-label={`${label} hex`} aria-invalid={invalid} value={draft}
        title="Six-digit hex color, or blank to follow Hermes skin"
        onChange={(e) => { setDraft(e.target.value); setInvalid(false); }} onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") { e.preventDefault(); commit(); }
          if (e.key === "Escape") { e.preventDefault(); setDraft(value); setInvalid(false); }
        }} />
      {invalid && <span className="color-error" role="status">Use six hex digits, e.g. #91c99b</span>}
      </div>
    </div>
  );
}

function SectionTitle({ children }) {
  return <h3 className="settings-section">{children}</h3>;
}

function EmbeddedTerminal({ active, onSession, onError, palette, fontSize, registerRestart }) {
  const host = useRef(null),
    fit = useRef(null),
    term = useRef(null),
    activeRef = useRef(active),
    syncSize = useRef(() => {}),
    runningRef = useRef(false),
    restartRef = useRef(() => {});
  activeRef.current = active;
  useEffect(() => {
    const terminal = new Terminal({
      cursorBlink: true,
      fontFamily: '"DejaVu Sans Mono", monospace',
      fontSize,
      scrollback: 5000,
      theme: paletteTools.terminalTheme(palette),
      allowProposedApi: false,
    });
    const fitter = new FitAddon();
    terminal.loadAddon(fitter);
    terminal.open(host.current);
    fit.current = fitter;
    term.current = terminal;
    let ready = false,
      queued = [],
      seq = 0,
      disposed = false;
    const offData = api.onData((packet) => {
      if (!ready) queued.push(packet);
      else if (packet.seq > seq) {
        terminal.write(packet.data);
        seq = packet.seq;
      }
    });
    const offExit = api.onExit(({ exitCode }) => {
      runningRef.current = false;
      terminal.write(`\r\n[Hermes exited: ${exitCode}. Use Restart for a fresh session.]\r\n`);
      onSession({ running: false });
    });
    const input = terminal.onData((data) => {
      if (!runningRef.current) return; // ignore keystrokes after exit
      api.input(data).catch(onError);
    });
    api
      .start()
      .then((snapshot) => {
        if (disposed) return;
        runningRef.current = snapshot.running;
        terminal.write(snapshot.data);
        seq = snapshot.seq;
        ready = true;
        queued
          .filter((p) => p.seq > seq)
          .forEach((p) => {
            terminal.write(p.data);
            seq = p.seq;
          });
        queued = [];
        onSession(snapshot);
        syncSize.current();
        terminal.focus();
      })
      .catch(onError);
    restartRef.current = async () => {
      const snapshot = await api.restart();
      if (disposed) return;
      runningRef.current = snapshot.running;
      terminal.reset();
      terminal.write(snapshot.data);
      seq = snapshot.seq;
      ready = true;
      queued = [];
      onSession(snapshot);
      syncSize.current();
      terminal.focus();
    };
    const resize = () => {
      if (!activeRef.current || !ready || host.current?.offsetHeight < 80) return;
      const size = fitter.proposeDimensions();
      if (!size || size.cols < 2 || size.rows < 2) return;
      terminal.resize(size.cols, size.rows);
      api.resize(size.cols, size.rows).catch(onError);
    };
    syncSize.current = resize;
    const observer = new ResizeObserver(resize);
    observer.observe(host.current);
    return () => {
      disposed = true;
      observer.disconnect();
      offData();
      offExit();
      input.dispose();
      terminal.dispose();
    };
  }, []);
  useEffect(() => {
    if (term.current) {
      term.current.options.fontSize = fontSize;
      term.current.options.theme = paletteTools.terminalTheme(palette);
    }
  }, [fontSize, palette]);
  useEffect(() => {
    if (active) {
      const t = setTimeout(() => {
        syncSize.current();
        term.current?.focus();
      }, 450);
      return () => clearTimeout(t);
    }
  }, [active]);
  useEffect(() => {
    registerRestart?.(() => restartRef.current());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return <div className="terminal" ref={host} aria-label="Hermes interactive terminal" />;
}

function Island() {
  const islandRef = useRef(null);
  const scheduleLayout = useRef(() => {});
  const previousLayout = useRef("");
  useEffect(() => {
    document.documentElement.dataset.overlay = String(api.overlay);
    document.documentElement.dataset.panelAnchor = String(Boolean(api.panelAnchor));
    if (api.overlay) document.title = "Hermes Island — panel overlay";
  }, []);
  const [palette, setPalette] = useState(paletteTools.fallback);
  useEffect(() => {
    let live = true;
    const off = api.onPalette((next) => live && setPalette(next));
    api.palette().then((next) => live && setPalette(next)).catch(() => {});
    return () => { live = false; off(); };
  }, []);
  const [cfg, setCfg] = useState(SETTINGS_DEFAULTS);
  useEffect(() => {
    api.settings().then((s) => setCfg((p) => ({ ...p, ...s }))).catch(() => {});
  }, []);
  // Keep the incoming skin intact so clearing an override follows live updates.
  const effectivePalette = useMemo(() => ({
    ...palette,
    accent: cfg.accent || palette.accent,
    background: cfg.background || palette.background,
  }), [palette, cfg.accent, cfg.background]);
  useEffect(() => {
    const root = document.documentElement;
    for (const [role, value] of Object.entries(effectivePalette))
      if (role !== "name") root.style.setProperty(`--skin-${role}`, value);
    root.dataset.skin = palette.name;
    root.dataset.font = cfg.uiFont;
    const border = cfg.borderEnabled ? (cfg.borderColor || "") : "transparent";
    if (border) root.style.setProperty("--island-border", border);
    else root.style.removeProperty("--island-border");
  }, [effectivePalette, cfg.borderEnabled, cfg.borderColor, cfg.uiFont]);
  // Phase 3 ownership seam: mode, view and `started` are the Rust actor's, never local.
  const island = useIslandState();
  const mode = toLegacyMode(island.mode);
  const view = island.view;
  const started = island.terminalStarted;
  const stateRev = useRef(island.rev);
  stateRev.current = island.rev;
  const [direction, setDirection] = useState(1);
  const viewIds = VIEWS.map((v) => v.id).filter((id) => id !== "monitor" || cfg.hardwareMonitor);
  const navigate = (dir) => {
    const idx = viewIds.indexOf(view);
    const next = idx + dir;
    if (next < 0 || next >= viewIds.length) return;
    setDirection(dir);
    dispatchIntent({ type: "selectView", view: viewIds[next] }).catch(fail);
  };
  useEffect(() => {
    let frame = null;
    const sync = () => {
      frame = null;
      const el = islandRef.current;
      if (el) {
        let b = el.getBoundingClientRect();
        // Translate the outer surface, never its contents. offsetLeft/Top are
        // the untransformed origin; clamp against the current animated size.
        const originX = el.offsetLeft, originY = el.offsetTop;
        const x = mode === "idle" ? 0 : Math.max(-originX, Math.min(innerWidth - originX - b.width, cfg.offsetX));
        const y = mode === "idle" ? 0 : Math.max(-originY, Math.min(innerHeight - originY - b.height, cfg.offsetY));
        const px = `${x}px`, py = `${y}px`;
        if (el.style.getPropertyValue("--popup-x") !== px || el.style.getPropertyValue("--popup-y") !== py) {
          el.style.setProperty("--popup-x", px);
          el.style.setProperty("--popup-y", py);
          b = el.getBoundingClientRect();
        }
        const rect = {x:Math.max(0, b.x), y:Math.max(0, b.y), width:b.width, height:b.height};
        rect.radius = parseFloat(getComputedStyle(el).borderRadius) || 0;
        const key = JSON.stringify([mode, devicePixelRatio, rect, stateRev.current]);
        if (api.overlay && key !== previousLayout.current) {
          previousLayout.current = key;
          api.layout(rect, mode, devicePixelRatio, stateRev.current).catch((e) => console.error(e));
        }
      }
    };
    // One coalesced measurement per geometry update, never an idle rAF loop.
    // Motion onUpdate covers width/height springs and radius-only animation;
    // ResizeObserver covers content, font and other non-animation size changes.
    const schedule = () => {
      if (frame === null) frame = requestAnimationFrame(sync);
    };
    scheduleLayout.current = schedule;
    const observer = new ResizeObserver(schedule);
    const el = islandRef.current;
    if (el) {
      observer.observe(el);
      el.querySelectorAll(".topline, .preview, .terminal-wrap").forEach((body) => observer.observe(body));
    }
    window.addEventListener("resize", schedule);
    sync();
    return () => {
      scheduleLayout.current = () => {};
      observer.disconnect();
      window.removeEventListener("resize", schedule);
      if (frame !== null) cancelAnimationFrame(frame);
    };
  }, [mode, view, island.rev, cfg.offsetX, cfg.offsetY, cfg.uiFont, cfg.showDate]);
  const [viewport, setViewport] = useState({ width: innerWidth, height: innerHeight });
  // Dynamic-island scaling: each view sizes the island to its own content,
  // mirroring upstream Ripple's per-tab widths/heights.
  const viewWidth = Math.min(view === "terminal" ? 1040 : 640, viewport.width - 64);
  // The standalone preview also needs room for its context heading and footer.
  const viewHeight = mode === "pinned" ? Math.max(0, Math.min(view === "terminal" ? 590 : view === "monitor" ? 360 : 490, viewport.height - (api.overlay ? 110 : 210))) : 0;
  useEffect(() => {
    const resize = () => setViewport({ width: innerWidth, height: innerHeight });
    window.addEventListener("resize", resize);
    return () => window.removeEventListener("resize", resize);
  }, []);
  const [time, setTime] = useState(new Date());
  const [voice, setVoice] = useState("checking");
  const [error, setError] = useState("");
  const [session, setSession] = useState({});
  const [busy, setBusy] = useState(false);
  const [stats, setStats] = useState(null);
  const restartRef = useRef(() => {});
  const fail = (e) => setError(e.message || String(e));
  useEffect(() => {
    // Align to the actual minute boundary so the panel and popup clocks agree.
    let timer;
    const schedule = () => {
      timer = setTimeout(() => { setTime(new Date()); schedule(); }, 60000 - Date.now() % 60000);
    };
    schedule();
    return () => clearTimeout(timer);
  }, []);
  const voiceRequest = useRef(null);
  useEffect(() => {
    if (mode === "idle") return;
    let live = true, timer;
    const poll = async () => {
      // Reuse an outstanding request across preview/pinned transitions. A slow
      // status call must never start overlapping subprocesses or IPC requests.
      if (!voiceRequest.current) {
        voiceRequest.current = api.voice("status").finally(() => { voiceRequest.current = null; });
      }
      try {
        const v = await voiceRequest.current;
        if (live) setVoice(v.state);
      } catch {
        if (live) setVoice("unavailable");
      } finally {
        if (live) timer = setTimeout(poll, 1000);
      }
    };
    poll();
    return () => { live = false; clearTimeout(timer); };
  }, [mode]);
  useEffect(() => {
    if (mode !== "pinned" || view !== "monitor") return;
    let live = true;
    const poll = () => api.stats().then((s) => live && setStats(s)).catch(() => {});
    poll();
    // second sample shortly after paint primes the CPU delta baseline
    const warm = setTimeout(poll, 350);
    const timer = setInterval(poll, 1500);
    return () => { live = false; clearTimeout(warm); clearInterval(timer); };
  }, [mode, view]);
  const enter = () => { dispatchIntent({ type: "pointerEnter" }).catch(fail); };
  const leave = () => { dispatchIntent({ type: "pointerLeave" }).catch(fail); };
  const collapse = () => { dispatchIntent({ type: "collapse" }).catch(fail); };
  useEffect(() => {
    const escape = (e) => {
      if (e.key === "Escape" && !e.defaultPrevented && !e.target.closest(".terminal")) collapse();
    };
    window.addEventListener("keydown", escape);
    return () => window.removeEventListener("keydown", escape);
  }, []);
  useEffect(() => {
    const keys = (e) => {
      if (mode !== "pinned" || e.defaultPrevented || e.target.closest(".terminal, input, select, textarea")) return;
      if (e.key === "ArrowRight") { e.preventDefault(); navigate(1); }
      else if (e.key === "ArrowLeft") { e.preventDefault(); navigate(-1); }
    };
    window.addEventListener("keydown", keys);
    return () => window.removeEventListener("keydown", keys);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode, view, cfg.hardwareMonitor]);
  const pin = () => { dispatchIntent({ type: "pin" }).catch(fail); };
  useEffect(() => subscribeCue((cue) => {
    if (cue.kind === "click") playClick(cfg.clickSounds);
  }), [cfg.clickSounds]);
  const toggle = async () => {
    setBusy(true);
    try {
      const v = await api.voice("toggle");
      setVoice(v.state);
    } catch (e) { fail(e); }
    finally { setBusy(false); }
  };
  const setSetting = (patch) => {
    setCfg((prev) => ({ ...prev, ...patch }));
    api.setSettings(patch).catch(fail);
    if (patch.showInTaskbar !== undefined) api.taskbar(Boolean(patch.showInTaskbar)).catch(fail);
  };
  const swipeStart = useRef(null), swipeMoved = useRef(false), suppressClick = useRef(false);
  const wheelAcc = useRef(0), wheelLock = useRef(false);
  const handleWheel = (e) => {
    if (mode !== "pinned" || wheelLock.current || e.target.closest(".terminal")) return;
    if (Math.abs(e.deltaY) > Math.abs(e.deltaX)) return;
    let delta = e.deltaX;
    if (e.deltaMode === 1) delta *= 40;
    if (e.deltaMode === 2) delta *= 800;
    wheelAcc.current += delta;
    if (Math.abs(wheelAcc.current) >= 60) {
      const dir = wheelAcc.current > 0 ? 1 : -1;
      wheelAcc.current = 0;
      wheelLock.current = true;
      navigate(dir);
      setTimeout(() => { wheelLock.current = false; }, 500);
    }
  };
  const handlePointerDown = (e) => {
    if (mode !== "pinned" || (e.pointerType === "mouse" && e.button !== 0)) return;
    if (e.target.closest(".terminal, button, input, textarea, select, label, a, [contenteditable]")) return;
    swipeStart.current = { x: e.clientX, y: e.clientY };
    swipeMoved.current = false;
  };
  const handlePointerMove = (e) => {
    if (!swipeStart.current) return;
    if (Math.abs(e.clientX - swipeStart.current.x) > 8) swipeMoved.current = true;
  };
  const handlePointerUp = (e) => {
    setTimeout(() => { suppressClick.current = false; }, 120);
    if (!swipeStart.current) return;
    const sx = swipeStart.current.x;
    swipeStart.current = null;
    if (!swipeMoved.current) return;
    const dx = e.clientX - sx;
    if (Math.abs(dx) < 60) return;
    suppressClick.current = true;
    navigate(dx < 0 ? 1 : -1);
  };
  const idleOpacity = (cfg.stealth && mode === "idle") ? 0.08 : 1;
  return (
    <main onClickCapture={(e) => {
      if (e.target.closest('button:not(:disabled):not([data-click-sound="manual"]):not([aria-label="Click sounds"])')) playClick(cfg.clickSounds);
    }}>
      <div className="context">
        <span className="mark">H</span>
        <div>HERMES <span className="muted">/ ISLAND LAB</span></div>
        <span className="tag">ISOLATED PREVIEW</span>
      </div>
      <div className="stage">
        <motion.section
          ref={islandRef}
          className={`island ${mode}`}
          data-state={mode}
          data-view={view}
          style={{ opacity: idleOpacity, transform: "translate(var(--popup-x, 0px), var(--popup-y, 0px))" }}
          onUpdate={() => scheduleLayout.current()}
          onAnimationComplete={() => scheduleLayout.current()}
          onClick={(event) => {
            if (suppressClick.current || event.target.closest("button, .terminal")) return;
            if (mode === "pinned" && event.target.closest(".topline")) {
              collapse();
            } else if (mode !== "pinned") pin();
          }}
          onMouseEnter={enter}
          onMouseLeave={leave}
          onWheel={handleWheel}
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={handlePointerUp}
          animate={{
            width: mode === "idle" ? 150 : mode === "preview" ? 420 : viewWidth,
            borderRadius: mode === "idle" ? 20 : mode === "preview" ? 28 : 24,
            backgroundColor: mode === "idle" ? `${cfg.background || palette.surface}E6` : effectivePalette.background,
          }}
          transition={UI_SPRING}
        >
          <div className="topline">
            {mode !== "idle" && <span className="beacon" />}
            <Control data-testid="clock" className="clock" onClick={pin} aria-label="Open Hermes terminal">
              <span className="clock-time">{time.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</span>
              {cfg.showDate && (
                <span className="clock-date">
                  {time.toLocaleDateString([], { weekday: "short", month: "short", day: "numeric" })}
                </span>
              )}
            </Control>
            {mode !== "idle" && <span className="session-label">{mode === "pinned" ? "HERMES · LIVE TERMINAL" : "HERMES"}</span>}
            {mode === "pinned" && (
              <Control data-testid="collapse" className="collapse" aria-label="Minimize island" onClick={collapse} title="Minimize into island (session keeps running)">−</Control>
            )}
          </div>
          <AnimatePresence initial={false}>
            {mode === "preview" && (
              <motion.div className="preview" initial={{ opacity: 0, height: 0 }} animate={{ opacity: 1, height: "auto" }} exit={{ opacity: 0, height: 0 }} transition={{ duration: 0.16 }}>
                <p className="preview-title">A quiet place to think.</p>
                <p className="hint">{session.pid ? "Your session is still here." : "Your real Hermes session, one click away."}</p>
                <div className="voice-row">
                  <Control data-testid="voice" disabled={busy || voice === "checking" || voice === "unavailable"} onClick={toggle}>
                    {voice === "muted" ? "◌" : "◉"} Voice {voice}
                  </Control>
                  <span>Shared mute</span>
                </div>
                <Control data-testid="open" className="open" onClick={pin}>
                  {started ? "Resume terminal" : "Open terminal"} <span>↗</span>
                </Control>
                <p className="footnote">Speech remains per-session opt-in: /voice tts</p>
              </motion.div>
            )}
          </AnimatePresence>
          <motion.div
            className="terminal-wrap"
            onUpdate={() => scheduleLayout.current()}
            onAnimationComplete={() => scheduleLayout.current()}
            style={{ visibility: mode === "pinned" ? "visible" : "hidden" }}
            animate={{ height: viewHeight, opacity: mode === "pinned" ? 1 : 0 }}
            transition={UI_SPRING}
            aria-hidden={mode !== "pinned"}
          >
            <div className="terminal-toolbar">
              <div className="view-tabs">
                {viewIds.map((id) => {
                  const v = VIEWS.find((x) => x.id === id);
                  return (
                    <Control key={id} data-testid={`view-${id}`} className={`view-tab ${view === id ? "active" : ""}`} onClick={() => { setDirection(viewIds.indexOf(id) > viewIds.indexOf(view) ? 1 : -1); dispatchIntent({ type: "selectView", view: id }).catch(fail); }}>
                      {v.label}
                    </Control>
                  );
                })}
              </div>
              <div className="toolbar-right">
                {!session.running && session.pid && (
                  <button className="restart" data-testid="restart" onClick={() => restartRef.current()}>⟳ Restart</button>
                )}
                {session.pid ? (
                  <span data-testid="pid">PTY {session.pid} · {session.running ? "running" : "exited"}</span>
                ) : (
                  <span data-testid="pid">Starting local Hermes…</span>
                )}
                <button data-testid="voice-pinned" onClick={toggle} disabled={busy || !["muted", "unmuted"].includes(voice)}>Voice {voice}</button>
              </div>
            </div>
            <div className="view-stage">
              <AnimatePresence mode="wait" initial={false}>
                <motion.div
                  key={view}
                  className="view-pane"
                  initial={{ x: direction > 0 ? 60 : -60, opacity: 0 }}
                  animate={{ x: 0, opacity: 1 }}
                  exit={{ x: direction > 0 ? -60 : 60, opacity: 0 }}
                  transition={{ duration: 0.16, x: { type: "spring", stiffness: 500, damping: 40 } }}
                >
                  {view === "terminal" && (
                    started ? (
                      <EmbeddedTerminal
                        active={mode === "pinned"}
                        palette={effectivePalette}
                        fontSize={cfg.terminalFontSize}
                        onSession={(s) => setSession((prev) => ({ ...prev, ...s }))}
                        onError={fail}
                        registerRestart={(fn) => { restartRef.current = fn; }}
                      />
                    ) : (
                      <div className="terminal-empty">
                        <p>Pin to start a local Hermes session.</p>
                      </div>
                    )
                  )}
                  {view === "monitor" && <Monitor stats={stats} />}
                  {view === "ai" && <AIStats />}
                  {view === "settings" && (
                    <div className="settings-panel">
                      <SectionTitle>General</SectionTitle>
                      <Toggle label="Launch at startup" hint="Linux XDG autostart" on={!!cfg.autostart} onChange={(v) => setSetting({ autostart: v })} />
                      <Toggle label="Show in taskbar" hint="Overlay window" on={!!cfg.showInTaskbar} onChange={(v) => setSetting({ showInTaskbar: v })} />
                      <Toggle label="Auto-hide when idle" hint="Stealth: ghost pill" on={!!cfg.stealth} onChange={(v) => setSetting({ stealth: v })} />
                      <Toggle label="Hover to preview" hint="Reveal on hover" on={cfg.hoverDelay > 0} onChange={(v) => setSetting({ hoverDelay: v ? 180 : 0 })} />
                      <Toggle label="Click sounds" hint="Quiet local synthesized feedback" on={!!cfg.clickSounds} onChange={(v) => { if (v) playClick(true); setSetting({ clickSounds: v }); }} />
                      <div className="setting-row">
                        <span className="setting-hint">Preview click feedback</span>
                        <Control data-click-sound="manual" disabled={!cfg.clickSounds} onClick={() => playClick(true)}>Test sound</Control>
                      </div>
                      {cfg.hoverDelay > 0 && <Slider label="Hover delay" min={0} max={600} step={20} value={cfg.hoverDelay} unit=" ms" onChange={(v) => setSetting({ hoverDelay: v })} />}
                      <SectionTitle>Appearance</SectionTitle>
                      <ColorPicker label="Accent color" value={cfg.accent} swatches={ACCENT_SWATCHES} onChange={(v) => setSetting({ accent: v })} />
                      <ColorPicker label="Background color" value={cfg.background} swatches={BG_SWATCHES} onChange={(v) => setSetting({ background: v })} />
                      <ColorPicker label="Panel clock background" value={cfg.panelClockColor} swatches={ACCENT_SWATCHES} onChange={(v) => setSetting({ panelClockColor: v })} />
                      <Toggle label="Island border" hint="Show the outline" on={!!cfg.borderEnabled} onChange={(v) => setSetting({ borderEnabled: v })} />
                      {cfg.borderEnabled && <ColorPicker label="Border color" value={cfg.borderColor} swatches={ACCENT_SWATCHES} onChange={(v) => setSetting({ borderColor: v })} />}
                      <Toggle label="UI font" hint="OpenRunde / system" on={cfg.uiFont === "openrunde"} onChange={(v) => setSetting({ uiFont: v ? "openrunde" : "system" })} />
                      <Toggle label="Show date under time" on={!!cfg.showDate} onChange={(v) => setSetting({ showDate: v })} />
                      <Slider label="Terminal font size" min={10} max={18} step={1} value={cfg.terminalFontSize} unit="px" onChange={(v) => setSetting({ terminalFontSize: v })} />
                      <SectionTitle>Island position</SectionTitle>
                      <Slider label="Opened bar offset X" min={-60} max={60} step={4} value={cfg.offsetX} unit=" px" onChange={(v) => setSetting({ offsetX: v })} />
                      <Slider label="Opened bar offset Y" min={0} max={100} step={4} value={cfg.offsetY} unit=" px" onChange={(v) => setSetting({ offsetY: v })} />
                      <p className="settings-note">Moves the whole popup, including its header and background. The panel pill stays fixed; the popup is kept on-screen.</p>
                      <SectionTitle>Widgets</SectionTitle>
                      <Toggle label="Hardware monitor" hint="System Monitor view" on={!!cfg.hardwareMonitor} onChange={(v) => setSetting({ hardwareMonitor: v })} />
                      <p className="settings-note">Swipe left/right (or arrow keys) to switch between Terminal, Monitor, AI stats and Settings.</p>
                    </div>
                  )}
                </motion.div>
              </AnimatePresence>
            </div>
          </motion.div>
        </motion.section>
        {error && <p role="alert" className="error">{error}</p>}
      </div>
      <footer>
        <span>HOVER TO PREVIEW <b>·</b> CLICK TO PIN <b>·</b> MINIMIZE WITHOUT DISCONNECTING</span>
        <span>No panel changes. No autostart. Local PTY.</span>
      </footer>
    </main>
  );
}

createRoot(document.getElementById("root")).render(<Island />);
