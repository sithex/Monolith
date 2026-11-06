// Pending Hermes approval/clarify prompt, mirrored by Rust (services/approvals). Latest-value
// topic: the `rev` guard keeps whichever snapshot is newest, events and the command reply alike.
import { useSyncExternalStore } from "react";
import { approvalAnswer as answerCommand, approvalGet, onApprovalChanged } from "./client";
import type { ApprovalChanged } from "./bindings";
import { createTopic } from "../stores/topic";

const topic = createTopic<ApprovalChanged | null>();
let started = false;

function apply(next: ApprovalChanged) { topic.apply(next, next.rev); }

/** Subscribe first, then read, so nothing between the two is lost (the rev guard orders them). */
export async function listenApprovals(): Promise<void> {
  if (started) return;
  started = true;
  try {
    topic.seed(null, 0);
    await onApprovalChanged(apply);
    apply(await approvalGet());
  } catch (error) { started = false; console.warn("approval topic unavailable", error); }
}

// Start as soon as the island page loads, so a prompt that arrives while collapsed is known
// before the hover notice mounts.
void listenApprovals();

export function useApproval(): ApprovalChanged | null {
  return useSyncExternalStore(topic.subscribe, () => { try { return topic.getSnapshot(); } catch { return null; } });
}

/** Answer the prompt the user saw. Rejections carry the backend's reason. */
export async function answerApproval(promptId: string, choice: string): Promise<void> {
  apply(await answerCommand({ promptId, choice }));
}
