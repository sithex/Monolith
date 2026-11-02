import { StrictMode, useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { reportGeometry } from "./ipc/client";
import "./styles/phase1.css";

// Phase 1 harness: three fixed rounded rects stand in for the real surfaces. The mode is local
// state driven by a debug key; from Phase 3 the Rust state actor owns it. The geometry contract
// (measure the outer element, report once per frame, never poll) is already the final one.
type Mode = "still" | "hover" | "large";
const MODES: readonly Mode[] = ["still", "hover", "large"];
const SHAPES: Record<Mode, { width: number; height: number; radius: number }> = {
  still: { width: 150, height: 36, radius: 18 },
  hover: { width: 420, height: 150, radius: 28 },
  large: { width: 1040, height: 700, radius: 24 },
};
const TOP = 8;

const KEYS: Record<string, (current: Mode) => Mode> = {
  m: (current) => MODES[(MODES.indexOf(current) + 1) % MODES.length] ?? "still",
  "1": () => "still",
  "2": () => "hover",
  "3": () => "large",
};

function Island() {
  const [mode, setMode] = useState<Mode>("still");
  const [error, setError] = useState<string | null>(null);
  const element = useRef<HTMLDivElement>(null);
  const rev = useRef(0);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const next = KEYS[event.key];
      if (next && !event.repeat && !event.ctrlKey && !event.altKey && !event.metaKey) {
        setMode(next);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const report = useCallback((current: Mode) => {
    const node = element.current;
    if (!node) return;
    const box = node.getBoundingClientRect();
    rev.current = (rev.current + 1) >>> 0;
    reportGeometry({
      stateRev: rev.current,
      mode: current,
      rects: [{ x: box.x, y: box.y, width: box.width, height: box.height, radius: SHAPES[current].radius }],
      scale: window.devicePixelRatio,
      settled: true, // no animation in Phase 1
    })
      .then(() => setError(null))
      .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)));
  }, []);

  // One measurement per committed mode change, after layout, coalesced to a single frame.
  useLayoutEffect(() => {
    const frame = requestAnimationFrame(() => report(mode));
    return () => cancelAnimationFrame(frame);
  }, [mode, report]);

  const shape = SHAPES[mode];
  return (
    <div
      ref={element}
      className="island"
      data-state={mode}
      data-testid="island"
      data-error={error ?? undefined}
      style={{
        width: shape.width,
        height: shape.height,
        borderRadius: shape.radius,
        left: `calc(50% - ${shape.width / 2}px)`,
        top: TOP,
      }}
    >
      {mode} · press m
    </div>
  );
}

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");
createRoot(root).render(
  <StrictMode>
    <Island />
  </StrictMode>,
);
