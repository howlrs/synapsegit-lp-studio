import {
  SCHEMA_VERSION,
  isPreviewDiagnosticMessage,
  isPreviewStructureMessage,
  isPreviewTargetMessage,
  isScopedPreviewUrl,
  type PreviewActionMessage,
  type PreviewDiagnosticMessage,
  type PreviewMode,
  type PreviewStructureMessage,
  type PreviewTargetMessage,
  type TargetKind,
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
export const readPreviewTarget = (
  event: MessageEvent<unknown>,
  binding: BridgeBinding,
): PreviewTargetMessage | null => {
  if (
    event.origin !== binding.expectedOrigin ||
    event.source !== binding.expectedSource ||
    !isPreviewTargetMessage(event.data)
  ) {
    return null;
  }
  const target = event.data;
  if (
    target.channelId !== binding.channelId ||
    target.projectId !== binding.projectId ||
    target.snapshotId !== binding.snapshotId ||
    target.revisionId !== binding.revisionId
  ) {
    return null;
  }
  return target;
};

/**
 * Runtime node handles are valid only for the currently bound iframe. They
 * may drive a follow-up capture action but must never be persisted as Target
 * evidence or sent to the local AI context endpoint.
 */
export const readPreviewStructure = (
  event: MessageEvent<unknown>,
  binding: BridgeBinding,
): PreviewStructureMessage | null => {
  if (
    event.origin !== binding.expectedOrigin ||
    event.source !== binding.expectedSource ||
    !isPreviewStructureMessage(event.data)
  ) {
    return null;
  }
  const structure = event.data;
  if (
    structure.channelId !== binding.channelId ||
    structure.projectId !== binding.projectId ||
    structure.snapshotId !== binding.snapshotId ||
    structure.revisionId !== binding.revisionId
  ) {
    return null;
  }
  return structure;
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
  targetKind: TargetKind,
  previewScale: number,
): void => {
  const message: PreviewActionMessage = {
    type: "synapsegit-lp.action",
    schemaVersion: SCHEMA_VERSION,
    channelId: binding.channelId,
    action: "set_mode",
    mode,
    targetKind,
    previewScale,
    projectId: binding.projectId,
    snapshotId: binding.snapshotId,
    revisionId: binding.revisionId,
  };
  target.postMessage(message, binding.expectedOrigin);
};

const postBoundAction = (
  target: Window,
  binding: Omit<BridgeBinding, "expectedSource">,
  action: "request_structure" | "capture_page",
): void => {
  const message: PreviewActionMessage = {
    type: "synapsegit-lp.action",
    schemaVersion: SCHEMA_VERSION,
    channelId: binding.channelId,
    action,
    projectId: binding.projectId,
    snapshotId: binding.snapshotId,
    revisionId: binding.revisionId,
  };
  target.postMessage(message, binding.expectedOrigin);
};

export const postRequestStructure = (
  target: Window,
  binding: Omit<BridgeBinding, "expectedSource">,
): void => postBoundAction(target, binding, "request_structure");

export const postCapturePage = (
  target: Window,
  binding: Omit<BridgeBinding, "expectedSource">,
): void => postBoundAction(target, binding, "capture_page");

export const postCaptureNode = (
  target: Window,
  binding: Omit<BridgeBinding, "expectedSource">,
  runtimeNodeHandle: string,
  targetKind: TargetKind,
): void => {
  const message: PreviewActionMessage = {
    type: "synapsegit-lp.action",
    schemaVersion: SCHEMA_VERSION,
    channelId: binding.channelId,
    action: "capture_node",
    runtimeNodeHandle,
    targetKind,
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
