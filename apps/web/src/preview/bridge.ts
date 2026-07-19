import {
  SCHEMA_VERSION,
  isPreviewDiagnosticMessage,
  isPreviewSelectionMessage,
  isScopedPreviewUrl,
  type PreviewActionMessage,
  type PreviewDiagnosticMessage,
  type PreviewMode,
  type PreviewSelectionMessage,
} from "@synapsegit-lp/contracts";

export interface BridgeBinding {
  expectedOrigin: string;
  expectedSource: Window;
  channelId: string;
  projectId: string;
  snapshotId: string;
  revisionId: string;
}

export const createChannelId = (): string => crypto.randomUUID();

/**
 * Checks the browser-controlled event metadata and the versioned channel
 * envelope. A passing value is still only an untrusted selection draft and
 * must be validated by the local server before it becomes a Target.
 */
export const readPreviewSelection = (
  event: MessageEvent<unknown>,
  binding: BridgeBinding,
): PreviewSelectionMessage | null => {
  if (
    event.origin !== binding.expectedOrigin ||
    event.source !== binding.expectedSource ||
    !isPreviewSelectionMessage(event.data)
  ) {
    return null;
  }
  const selection = event.data;
  if (
    selection.channelId !== binding.channelId ||
    selection.projectId !== binding.projectId ||
    selection.snapshotId !== binding.snapshotId ||
    selection.revisionId !== binding.revisionId
  ) {
    return null;
  }
  return selection;
};

/**
 * Reads only the privacy-safe diagnostic vocabulary. Preview code cannot send
 * raw URLs, exception messages, or stacks through this envelope.
 */
export const readPreviewDiagnostic = (
  event: MessageEvent<unknown>,
  binding: BridgeBinding,
): PreviewDiagnosticMessage | null => {
  if (
    event.origin !== binding.expectedOrigin ||
    event.source !== binding.expectedSource ||
    !isPreviewDiagnosticMessage(event.data)
  ) {
    return null;
  }
  const diagnostic = event.data;
  if (
    diagnostic.channelId !== binding.channelId ||
    diagnostic.projectId !== binding.projectId ||
    diagnostic.snapshotId !== binding.snapshotId ||
    diagnostic.revisionId !== binding.revisionId
  ) {
    return null;
  }
  return diagnostic;
};

export const postPreviewMode = (
  target: Window,
  binding: Omit<BridgeBinding, "expectedSource">,
  mode: PreviewMode,
): void => {
  const message: PreviewActionMessage = {
    type: "synapsegit-lp.action",
    schemaVersion: SCHEMA_VERSION,
    channelId: binding.channelId,
    action: "set_mode",
    mode,
    projectId: binding.projectId,
    snapshotId: binding.snapshotId,
    revisionId: binding.revisionId,
  };
  target.postMessage(message, binding.expectedOrigin);
};

export const postClearSelection = (
  target: Window,
  binding: Omit<BridgeBinding, "expectedSource">,
): void => {
  const message: PreviewActionMessage = {
    type: "synapsegit-lp.action",
    schemaVersion: SCHEMA_VERSION,
    channelId: binding.channelId,
    action: "clear_selection",
    projectId: binding.projectId,
    snapshotId: binding.snapshotId,
    revisionId: binding.revisionId,
  };
  target.postMessage(message, binding.expectedOrigin);
};

export const isAllowedPreviewUrl = (
  previewUrl: string,
  previewScopeBaseOrigin: string,
): boolean => isScopedPreviewUrl(previewUrl, previewScopeBaseOrigin);
