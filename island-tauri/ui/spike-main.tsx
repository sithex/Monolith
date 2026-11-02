import { animate } from "framer-motion";
import type { GeometryReport } from "./ipc/client";
import { reportGeometry } from "./ipc/client";
import "./styles/spike.css";

// SPEC §8 Phase 2.5 spike page (TEST ONLY; loaded solely by `--spike-panel`, never by the product).
//
// One rounded rect morphs from the stand-in pill's exact rectangle (panel height) to the hover
// size with the production spring (SPEC §6: stiffness 350, damping 32, mass 0.8). The page is
// driven by the nested-KWin test through `window.__spike` (reached over the test-control socket);
// real pointer input still arrives through KWin's EIS, never through this API.
//
// Geometry goes through the *production* path: `island_report_geometry` once per animation frame
// (coalesced in the rAF callback, never a perpetual loop) and a settled report at the end.

interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
  r: number;
}

// Window coordinates (window origin = screen (210, 0) in the 1600x1000 sandbox, so x = 515 is
// the screen-centred 150 px pill and y = 4 is the 4 px margin of a 44 px panel).
const PILL: Rect = { x: 515, y: 4, w: 150, h: 36, r: 18 };
const HOVER: Rect = { x: 380, y: 4, w: 420, h: 150, r: 28 };
const SPRING = { type: "spring", stiffness: 350, damping: 32, mass: 0.8 } as const;

const lerp = (a: number, b: number, t: number): number => a + (b - a) * t;
const rectAt = (t: number): Rect => ({
  x: lerp(PILL.x, HOVER.x, t),
  y: lerp(PILL.y, HOVER.y, t),
  w: lerp(PILL.w, HOVER.w, t),
  h: lerp(PILL.h, HOVER.h, t),
  r: lerp(PILL.r, HOVER.r, t),
});

export interface MorphResult {
  direction: "open" | "close";
  /** `Date.now()` when the spring started (correlates with the capture helper's `time.time()`). */
  startEpochMs: number;
  durationMs: number;
  /** requestAnimationFrame timestamp deltas inside the morph (ms), first frame excluded. */
  deltas: number[];
  reports: number;
  /** send → ack latency of every geometry report (ms). */
  ackMs: number[];
  errors: string[];
}

const island = document.createElement("div");
island.className = "spike-island";
island.dataset.testid = "spike-island";
document.getElementById("root")?.appendChild(island);

let progress = 0;
let rev = 0;
let lastResult: MorphResult | null = null;
const counters = {
  pointerdown: 0,
  pointerup: 0,
  click: 0,
  pointerenter: 0,
  pointerleave: 0,
  keydown: 0,
};
for (const name of ["pointerdown", "pointerup", "click", "pointerenter", "pointerleave"] as const) {
  island.addEventListener(name, () => {
    counters[name] += 1;
  });
}
// Keyboard reachability (the pinned terminal needs it): counted on the document, never submitted.
window.addEventListener("keydown", () => {
  counters.keydown += 1;
});

function paint(t: number): void {
  const r = rectAt(t);
  island.style.left = `${r.x}px`;
  island.style.top = `${r.y}px`;
  island.style.width = `${r.w}px`;
  island.style.height = `${r.h}px`;
  island.style.borderRadius = `${r.r}px`;
}

function report(
  stateRev: number,
  t: number,
  settled: boolean,
  mode: "still" | "hover",
): GeometryReport {
  const r = rectAt(t);
  return {
    stateRev,
    // Still + the native-pill policy => an EMPTY input region (the stand-in panel owns the pill).
    mode,
    rects: [{ x: r.x, y: r.y, width: r.w, height: r.h, radius: r.r }],
    scale: window.devicePixelRatio,
    settled,
  };
}

const sleep = (ms: number): Promise<void> => new Promise((done) => setTimeout(done, ms));

function morph(direction: "open" | "close", sendReports: boolean): Promise<MorphResult> {
  const target = direction === "open" ? 1 : 0;
  rev = (rev + 1) >>> 0;
  const myRev = rev;
  const deltas: number[] = [];
  const ackMs: number[] = [];
  const errors: string[] = [];
  const pending: Promise<unknown>[] = [];
  let reports = 0;
  let dirty = true;
  let done = false;
  let tail = 2; // two more frames after the spring settles
  let previous = -1;
  const startEpochMs = Date.now();
  const began = performance.now();

  const send = (t: number, settled: boolean): void => {
    reports += 1;
    const sentAt = performance.now();
    const mode = direction === "open" ? "hover" : "still";
    pending.push(
      reportGeometry(report(myRev, t, settled, mode))
        .then(() => {
          ackMs.push(performance.now() - sentAt);
        })
        .catch((e: unknown) => {
          errors.push(e instanceof Error ? e.message : String(e));
        }),
    );
  };

  return new Promise((resolve) => {
    const frame = (stamp: number): void => {
      if (previous >= 0) deltas.push(stamp - previous);
      previous = stamp;
      if (dirty && sendReports && !done) {
        dirty = false;
        send(progress, false);
      }
      if (done && tail-- <= 0) {
        const durationMs = performance.now() - began;
        void Promise.all(pending).then(() => {
          const result: MorphResult = {
            direction,
            startEpochMs,
            durationMs,
            deltas,
            reports,
            ackMs,
            errors,
          };
          lastResult = result;
          resolve(result);
        });
        return;
      }
      requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
    animate(progress, target, {
      ...SPRING,
      onUpdate: (value: number) => {
        progress = value;
        paint(value);
        dirty = true;
      },
      onComplete: () => {
        progress = target;
        paint(target);
        done = true;
        // The settled report is always sent: it applies the final region (also when the
        // per-frame reports are switched off for the baseline measurement).
        send(target, true);
      },
    });
  });
}

async function cycles(count: number, sendReports: boolean): Promise<MorphResult[]> {
  const out: MorphResult[] = [];
  for (let i = 0; i < count; i += 1) {
    out.push(await morph("open", sendReports));
    await sleep(150);
    out.push(await morph("close", sendReports));
    await sleep(150);
  }
  return out;
}

function webglRenderer(): string {
  const gl = document.createElement("canvas").getContext("webgl");
  if (!gl) return "no-webgl";
  const info = gl.getExtension("WEBGL_debug_renderer_info");
  return info ? String(gl.getParameter(info.UNMASKED_RENDERER_WEBGL)) : "no-debug-info";
}

const api = {
  pill: PILL,
  hover: HOVER,
  open: (sendReports: boolean) => morph("open", sendReports),
  close: (sendReports: boolean) => morph("close", sendReports),
  /** Starts the morph and returns at once (the capture burst runs while it animates). */
  startOpen: (sendReports: boolean): boolean => {
    lastResult = null;
    void morph("open", sendReports);
    return true;
  },
  startClose: (sendReports: boolean): boolean => {
    lastResult = null;
    void morph("close", sendReports);
    return true;
  },
  last: (): MorphResult | null => lastResult,
  cycles,
  setAlpha: (alpha: number): boolean => {
    document.documentElement.style.setProperty("--spike-alpha", String(alpha));
    return true;
  },
  state: () => ({
    progress,
    rev,
    rect: rectAt(progress),
    counters: { ...counters },
    dpr: window.devicePixelRatio,
    hasFocus: document.hasFocus(),
  }),
  webgl: webglRenderer,
};

declare global {
  interface Window {
    __spike: typeof api;
  }
}
window.__spike = api;

paint(0);
// Start state: the pill rect, Still => the native-pill policy gives an EMPTY input region.
rev = 1;
void reportGeometry(report(rev, 0, true, "still"))
  .then(() => {
    island.dataset.ready = "1";
  })
  .catch((e: unknown) => {
    island.dataset.error = e instanceof Error ? e.message : String(e);
  });
