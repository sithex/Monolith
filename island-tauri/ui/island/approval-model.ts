// Pure presentation rules for the approval notice (no React, no IPC): sizes, which buttons may
// be pressed how, and when a click is too early to trust. Tested in phase7b-approve.test.mjs.
import type { ApprovalChanged, ApprovalPrompt } from '../ipc/bindings';

export const HOVER_WIDTH = 420;
export const PLAIN_HOVER_WIDTH = 360;
export const PLAIN_HOVER_HEIGHT = 116;
/** Clicks this soon after a prompt appears are ignored: the pointer may already have been
 *  resting where a button appears (the click was aimed at something else). */
export const ARM_DELAY_MS = 450;
const CHROME = 42;          // Topline strip above the notice
const COLLAPSED_LINES = 3;
const EXPANDED_LINES = 9;
const LINE = 16;

export type NoticeView =
  | { kind: 'prompt'; prompt: ApprovalPrompt }
  | { kind: 'waiting' };

/** What the hover notice should show for this topic value, if anything. */
export function noticeFor(change: ApprovalChanged | null): NoticeView | null {
  if (!change) return null;
  if (change.prompt) return { kind: 'prompt', prompt: change.prompt };
  return change.waitingUnknown ? { kind: 'waiting' } : null;
}

/** Longer than the collapsed clamp can show (or cut by the backend): offer "Show all". */
export function needsExpand(text: string): boolean {
  return text.split('\n').length > COLLAPSED_LINES || text.length > 150;
}

/** Hover surface height for a notice. The surface grows to fit instead of scrolling. */
export function noticeHeight(view: NoticeView | null, expanded: boolean): number {
  if (!view) return PLAIN_HOVER_HEIGHT;
  if (view.kind === 'waiting') return CHROME + 96;
  const p = view.prompt;
  const textLines = expanded ? EXPANDED_LINES : Math.min(COLLAPSED_LINES, Math.max(1, p.text.split('\n').length));
  const expandRow = needsExpand(p.text) || p.textTruncated ? 22 : 0;
  const desc = p.description ? 20 : 0;
  const choices = p.answerable ? (p.kind === 'clarify' ? p.choices.length * 34 : 40) : 0;
  const note = !p.answerable ? 40 : 0;
  return CHROME + 14 + 24 + textLines * LINE + 22 + expandRow + desc + choices + note + 14;
}

/** Choices that approve or pick something need a real pointer click; Deny may be keyboarded. */
export function pointerOnly(style: string): boolean {
  return style !== 'deny';
}

/**
 * May this click answer? `detail` is the DOM click count: 0 means the click came from the
 * keyboard (Enter/Space on a focused button), which never approves anything.
 */
export function canPress(args: { style: string; detail: number; armedAt: number; now: number; prompt: ApprovalPrompt; pending: boolean }): boolean {
  const { style, detail, armedAt, now, prompt, pending } = args;
  if (pending || !prompt.answerable || prompt.answered) return false;
  if (now < armedAt) return false;
  if (pointerOnly(style) && detail === 0) return false;
  return true;
}

export const KIND_LABEL: Record<string, string> = { approval: 'Hermes needs approval', clarify: 'Hermes has a question' };
