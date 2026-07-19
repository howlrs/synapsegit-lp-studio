import {
  isAllowedPreviewUrl,
  postPreviewMode,
  readPreviewSelection,
} from "../preview/bridge";

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
  expectedOrigin: "https://preview.test",
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
    origin: "https://preview.test",
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
        expectedOrigin: "https://preview.test",
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
      "https://preview.test",
    );
  });

  it("allows preview URLs only on the bootstrapped preview origin", () => {
    expect(
      isAllowedPreviewUrl(
        "https://preview.test/projects/project-001/index.html",
        "https://preview.test",
      ),
    ).toBe(true);
    expect(
      isAllowedPreviewUrl(
        "https://preview.test.attacker.example/project",
        "https://preview.test",
      ),
    ).toBe(false);
    expect(isAllowedPreviewUrl("javascript:alert(1)", "null")).toBe(false);
  });
});
