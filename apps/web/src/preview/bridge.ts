import {
  SCHEMA_VERSION,
  isPreviewSelectionMessage,
  type PreviewActionMessage,
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
  expectedOrigin: string,
): boolean => {
  try {
    const parsed = new URL(previewUrl);
    return (
      parsed.origin === expectedOrigin && /^https?:$/.test(parsed.protocol)
    );
  } catch {
    return false;
  }
};
