import {
  isAllowedPreviewUrl,
  postCaptureNode,
  postCapturePage,
  postPreviewMode,
  postRequestStructure,
  readPreviewDiagnostic,
  readPreviewStructure,
  readPreviewTarget,
} from "../preview/bridge";
import { targetResponseFixture } from "./fixtures";

const previewScopeBase = "http://localhost:4174";
const acceptedOrigin =
  "http://pv-11111111111111111111111111111111.localhost:4174";
const proposedOrigin =
  "http://pv-22222222222222222222222222222222.localhost:4174";

const source = {} as Window;
const message = {
  type: "synapsegit-lp.target-draft",
  schemaVersion: "1",
  channelId: "channel-123",
  projectId: "project-001",
  snapshotId: "proposal-001",
  revisionId: "revision-001",
  target: {
    ...targetResponseFixture.target,
    captureRevisionId: "revision-001",
    captureSource: "proposal",
    captureProposalId: "proposal-001",
  },
};
const binding = {
  expectedOrigin: proposedOrigin,
  expectedSource: source,
  channelId: "channel-123",
  projectId: "project-001",
  snapshotId: "proposal-001",
  revisionId: "revision-001",
};

const event = (
  overrides: Partial<MessageEvent<unknown>> = {},
): MessageEvent<unknown> =>
  ({
    origin: proposedOrigin,
    source,
    data: message,
    ...overrides,
  }) as MessageEvent<unknown>;

describe("untrusted preview bridge", () => {
  it("accepts only the expected origin, source, channel, version, and bindings", () => {
    expect(readPreviewTarget(event(), binding)).toEqual(message);
    expect(
      readPreviewTarget(event({ origin: "https://attacker.test" }), binding),
    ).toBeNull();
    expect(
      readPreviewTarget(event({ source: {} as Window }), binding),
    ).toBeNull();
    expect(
      readPreviewTarget(
        event({ data: { ...message, schemaVersion: "2" } }),
        binding,
      ),
    ).toBeNull();
    expect(
      readPreviewTarget(
        event({ data: { ...message, channelId: "other-channel" } }),
        binding,
      ),
    ).toBeNull();
    expect(
      readPreviewTarget(
        event({ data: { ...message, snapshotId: "other-proposal" } }),
        binding,
      ),
    ).toBeNull();
    expect(
      readPreviewTarget(
        event({ data: { ...message, revisionId: "stale-revision" } }),
        binding,
      ),
    ).toBeNull();
    expect(
      readPreviewTarget(
        event({
          data: {
            ...message,
            target: {
              ...targetResponseFixture.target,
              captureRevisionId: "revision-001",
            },
          },
        }),
        binding,
      ),
    ).toBeNull();
  });

  it("sends commands to an exact origin with versioned project bindings", () => {
    const postMessage = vi.fn();
    postPreviewMode(
      { postMessage } as unknown as Window,
      {
        expectedOrigin: proposedOrigin,
        channelId: "channel-123",
        projectId: "project-001",
        snapshotId: "proposal-001",
        revisionId: "revision-001",
      },
      "select",
      "element",
      1,
    );
    expect(postMessage).toHaveBeenCalledWith(
      {
        type: "synapsegit-lp.action",
        schemaVersion: "1",
        channelId: "channel-123",
        action: "set_mode",
        mode: "select",
        targetKind: "element",
        previewScale: 1,
        projectId: "project-001",
        snapshotId: "proposal-001",
        revisionId: "revision-001",
      },
      proposedOrigin,
    );
  });

  it("accepts a bound dynamic structure but keeps runtime handles outside targets", () => {
    const structure = {
      type: "synapsegit-lp.structure",
      schemaVersion: "1",
      channelId: "channel-123",
      projectId: "project-001",
      snapshotId: "proposal-001",
      revisionId: "revision-001",
      nodes: [
        {
          runtimeNodeHandle: "node-hero",
          kind: "block",
          label: "Hero section",
          tagName: "section",
          depth: 0,
        },
      ],
    };
    expect(readPreviewStructure(event({ data: structure }), binding)).toEqual(
      structure,
    );
    expect(
      readPreviewStructure(
        event({ data: { ...structure, snapshotId: "other" } }),
        binding,
      ),
    ).toBeNull();
    expect(JSON.stringify(message.target)).not.toContain("runtimeNodeHandle");
  });

  it("sends page, structure, and runtime-node capture actions to the bound origin", () => {
    const postMessage = vi.fn();
    const target = { postMessage } as unknown as Window;
    const actionBinding = {
      expectedOrigin: proposedOrigin,
      channelId: "channel-123",
      projectId: "project-001",
      snapshotId: "proposal-001",
      revisionId: "revision-001",
    };
    postRequestStructure(target, actionBinding);
    postCapturePage(target, actionBinding);
    postCaptureNode(target, actionBinding, "node-hero", "block");

    expect(postMessage).toHaveBeenNthCalledWith(
      1,
      expect.objectContaining({ action: "request_structure" }),
      proposedOrigin,
    );
    expect(postMessage).toHaveBeenNthCalledWith(
      2,
      expect.objectContaining({ action: "capture_page" }),
      proposedOrigin,
    );
    expect(postMessage).toHaveBeenNthCalledWith(
      3,
      expect.objectContaining({
        action: "capture_node",
        runtimeNodeHandle: "node-hero",
        targetKind: "block",
      }),
      proposedOrigin,
    );
  });

  it("allows only exact scoped origins under the bootstrapped local scope", () => {
    expect(
      isAllowedPreviewUrl(
        `${acceptedOrigin}/preview/project-001/revision-001/`,
        previewScopeBase,
      ),
    ).toBe(true);
    expect(
      isAllowedPreviewUrl(
        `${proposedOrigin}/preview/project-001/proposal-001/`,
        previewScopeBase,
      ),
    ).toBe(true);
    expect(
      isAllowedPreviewUrl(
        `${acceptedOrigin}/preview/project-001/revision-001/`,
        "http://attacker.localhost:4174",
      ),
    ).toBe(false);
    for (const url of [
      "http://pv-11111111111111111111111111111111.localhost.attacker.test:4174/preview/project-001/revision-001/",
      "http://child.pv-11111111111111111111111111111111.localhost:4174/preview/project-001/revision-001/",
      `${acceptedOrigin.replace(":4174", ":4175")}/preview/project-001/revision-001/`,
      "http://user@pv-11111111111111111111111111111111.localhost:4174/preview/project-001/revision-001/",
      `${acceptedOrigin}/preview/project-001/../private/`,
      `${acceptedOrigin}/preview/project-001/revision-001/?secret=1`,
      "javascript:alert(1)",
    ]) {
      expect(isAllowedPreviewUrl(url, previewScopeBase)).toBe(false);
    }
  });

  it("accepts diagnostics only from the exact bound source and snapshot", () => {
    const diagnostic = {
      type: "synapsegit-lp.diagnostic",
      schemaVersion: "1",
      channelId: "channel-123",
      projectId: "project-001",
      snapshotId: "proposal-001",
      revisionId: "revision-001",
      severity: "error",
      code: "site_error",
      sourceUnavailable: true,
    };
    expect(readPreviewDiagnostic(event({ data: diagnostic }), binding)).toEqual(
      diagnostic,
    );
    expect(
      readPreviewDiagnostic(
        event({ origin: acceptedOrigin, data: diagnostic }),
        binding,
      ),
    ).toBeNull();
    expect(
      readPreviewDiagnostic(
        event({ source: {} as Window, data: diagnostic }),
        binding,
      ),
    ).toBeNull();
    expect(
      readPreviewDiagnostic(
        event({ data: { ...diagnostic, snapshotId: "accepted-revision" } }),
        binding,
      ),
    ).toBeNull();
    expect(
      readPreviewDiagnostic(
        event({ data: { ...diagnostic, stack: "private stack" } }),
        binding,
      ),
    ).toBeNull();
  });
});
