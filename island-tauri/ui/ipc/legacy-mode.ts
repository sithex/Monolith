// The only legacy vocabulary translation, shared by the compatibility shim and its tests.
import type { IslandMode } from "./bindings";
export type LegacyMode = "idle" | "preview" | "pinned";
const TO_CANONICAL: Record<LegacyMode, IslandMode> = { idle: "still", preview: "hover", pinned: "large" };
const TO_LEGACY: Record<IslandMode, LegacyMode> = { still: "idle", hover: "preview", large: "pinned" };
export function toCanonicalMode(mode: unknown): IslandMode {
  if (typeof mode === "string" && Object.hasOwn(TO_CANONICAL, mode)) return TO_CANONICAL[mode as LegacyMode];
  throw new Error("Invalid overlay layout");
}
export const toLegacyMode = (mode: IslandMode): LegacyMode => TO_LEGACY[mode];
