// The canonical generated command keeps the read-only bridge typed end-to-end.
import { claudeCodeRaw as readRaw } from '../ipc/client';
import type { ClaudeCodeRaw, ClaudeCodeRequest } from './claude-code-model';
export type { ClaudeCodeCursor, ClaudeCodeRaw, ClaudeCodeRequest, ClaudeFile, ClaudeRegEntry, Fact } from './claude-code-model';
export const claudeCodeRaw = (req: ClaudeCodeRequest): Promise<ClaudeCodeRaw> => readRaw(req);
