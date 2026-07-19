import {
  isAllowedPreviewUrl,
  postPreviewMode,
  readPreviewDiagnostic,
  readPreviewSelection,
} from "../preview/bridge";

const previewScopeBase = "http://localhost:4174";
const acceptedOrigin =
  "http://pv-11111111111111111111111111111111.localhost:4174";
const proposedOrigin =
  "http://pv-22222222222222222222222222222222.localhost:4174";

const source = {} as Window;
const message = {
  type: "synapsegit-lp.selection",
  schemaVersion: "1",
  channelId: "channel-123",
  projectId: "project-001",
  snapshotId: "proposal-001",
  revisionId: "revision-001",
  elementId: "hero-heading",
  rect: { x: 20, y: 30, width: 240, height: 60 },
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
    expect(readPreviewSelection(event(), binding)).toEqual(message);
    expect(
      readPreviewSelection(event({ origin: "https://attacker.test" }), binding),
    ).toBeNull();
    expect(
      readPreviewSelection(event({ source: {} as Window }), binding),
    ).toBeNull();
    expect(
      readPreviewSelection(
        event({ data: { ...message, schemaVersion: "2" } }),
        binding,
      ),
    ).toBeNull();
    expect(
      readPreviewSelection(
        event({ data: { ...message, channelId: "other-channel" } }),
        binding,
      ),
    ).toBeNull();
    expect(
      readPreviewSelection(
        event({ data: { ...message, snapshotId: "other-proposal" } }),
        binding,
      ),
    ).toBeNull();
    expect(
      readPreviewSelection(
        event({ data: { ...message, revisionId: "stale-revision" } }),
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
    );
    expect(postMessage).toHaveBeenCalledWith(
      {
        type: "synapsegit-lp.action",
        schemaVersion: "1",
        channelId: "channel-123",
        action: "set_mode",
        mode: "select",
        projectId: "project-001",
        snapshotId: "proposal-001",
        revisionId: "revision-001",
      },
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
