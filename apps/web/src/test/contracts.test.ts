import Ajv2020 from "ajv/dist/2020.js";
import {
  TARGET_CONTRACT_LIMITS,
  isAiAttemptStatusV1,
  isAiProviderAttributionV1,
  isAiProviderErrorV1,
  isAiProviderResultV1,
  isAiProviderStreamEventV1,
  isApiErrorResponse,
  isApprovalResponse,
  isBootstrapResponse,
  isChangeSetV1,
  isContextManifestV1,
  isContextResponse,
  isCreateContextRequest,
  isDecisionResponse,
  isExportResponse,
  isImportPreviewResponse,
  isPreviewActionMessage,
  isPreviewDiagnosticMessage,
  isPreviewScopeBaseOrigin,
  isPreviewSelectionMessage,
  isProjectResponse,
  isProjectsResponse,
  isProposalResponse,
  isScopedPreviewUrl,
  isTargetResolverResultV1,
  isTargetResponse,
  isTargetV1,
  type TargetResolverCandidateV1,
  type TargetResolverResultV1,
  type TargetV1,
} from "@synapsegit-lp/contracts";
import apiSchema from "../../../../packages/contracts/schemas/api-v1.schema.json";
import {
  apiErrorResponseFixture,
  allOperationsChangeSetFixture,
  approvalResponseFixture,
  bootstrapFixture,
  contextResponseFixture,
  decisionResponseFixture,
  exportResponseFixture,
  importPreviewResponseFixture,
  projectResponseFixture,
  projectsResponseFixture,
  proposalResponseFixture,
  targetResponseFixture,
} from "./fixtures";

type Guard = (value: unknown) => boolean;

const previewSelection = {
  type: "synapsegit-lp.selection",
  schemaVersion: "1",
  channelId: "channel-001",
  projectId: "project-001",
  snapshotId: "proposal-001",
  revisionId: "revision-accepted-001",
  elementId: "hero-copy",
  rect: { x: 1, y: 2, width: 300, height: 80 },
};

const previewSetMode = {
  type: "synapsegit-lp.action",
  schemaVersion: "1",
  channelId: "channel-001",
  action: "set_mode",
  mode: "select",
  targetKind: "element",
  previewScale: 1,
  projectId: "project-001",
  snapshotId: "proposal-001",
  revisionId: "revision-accepted-001",
};

const previewClearSelection = {
  type: "synapsegit-lp.action",
  schemaVersion: "1",
  channelId: "channel-001",
  action: "clear_selection",
  projectId: "project-001",
  snapshotId: "proposal-001",
  revisionId: "revision-accepted-001",
};

const previewDiagnostic = {
  type: "synapsegit-lp.diagnostic",
  schemaVersion: "1",
  channelId: "channel-001",
  projectId: "project-001",
  snapshotId: "proposal-001",
  revisionId: "revision-accepted-001",
  severity: "warning",
  code: "csp_blocked",
  sourceUnavailable: true,
};

const targetViewportV1 = {
  cssWidth: 1440,
  cssHeight: 900,
  scrollX: 0,
  scrollY: 120,
  devicePixelRatio: 2,
  visualViewportScale: 1,
  previewScale: 0.8,
};

const targetDocumentV1 = {
  cssWidth: 1440,
  cssHeight: 4200,
  layoutEpoch: 7,
};

const targetGeometryV1 = {
  documentCssPixelRect: { x: 120, y: 320, width: 600, height: 120 },
  viewportCssPixelRect: { x: 120, y: 200, width: 600, height: 120 },
  viewportNormalizedRect: {
    x: 120 / 1440,
    y: 200 / 900,
    width: 600 / 1440,
    height: 120 / 900,
  },
};

const elementAnchorV1 = {
  tagName: "H1",
  uniqueElementId: "hero-title",
  role: "heading",
  accessibleName: "Build a focused landing page",
  domPath: "main:nth-child(1)>section:nth-child(1)>h1:nth-child(1)",
  classTokens: ["hero-title", "display-xl"],
  ancestorFingerprint: "main/section.hero",
  siblingIndex: 0,
};

const targetV1Fixtures = [
  {
    schemaVersion: 1,
    targetId: "target-page-001",
    captureRevisionId: "revision-accepted-001",
    captureSource: "accepted",
    pagePath: "index.html",
    kind: "page",
    label: "Landing page",
    viewport: targetViewportV1,
    document: targetDocumentV1,
  },
  {
    schemaVersion: 1,
    targetId: "target-block-001",
    captureRevisionId: "revision-accepted-001",
    captureSource: "accepted",
    pagePath: "index.html",
    kind: "block",
    label: "Hero section",
    viewport: targetViewportV1,
    document: targetDocumentV1,
    geometry: targetGeometryV1,
    elementAnchor: { ...elementAnchorV1, tagName: "SECTION" },
    block: { source: "semantic", level: 1 },
  },
  {
    schemaVersion: 1,
    targetId: "target-element-001",
    captureRevisionId: "revision-accepted-001",
    captureSource: "accepted",
    pagePath: "index.html",
    kind: "element",
    label: "Hero heading",
    viewport: targetViewportV1,
    document: targetDocumentV1,
    geometry: targetGeometryV1,
    elementAnchor: elementAnchorV1,
  },
  {
    schemaVersion: 1,
    targetId: "target-text-001",
    captureRevisionId: "revision-proposal-001",
    captureSource: "proposal",
    captureProposalId: "proposal-001",
    pagePath: "index.html",
    kind: "text",
    label: "Hero heading text",
    viewport: targetViewportV1,
    document: targetDocumentV1,
    geometry: targetGeometryV1,
    elementAnchor: elementAnchorV1,
    textAnchor: {
      exact: "focused landing page",
      prefix: "Build a ",
      suffix: " today",
      startOffset: 8,
      endOffset: 28,
    },
  },
  {
    schemaVersion: 1,
    targetId: "target-point-001",
    captureRevisionId: "revision-accepted-001",
    captureSource: "accepted",
    pagePath: "index.html",
    kind: "point",
    label: "Space below hero heading",
    viewport: targetViewportV1,
    document: targetDocumentV1,
    point: {
      documentCssPixel: { x: 720, y: 520 },
      viewportNormalized: { x: 0.5, y: 400 / 900 },
    },
    regionAnchor: {
      containingBlock: { ...elementAnchorV1, tagName: "SECTION" },
      previousVisibleSibling: elementAnchorV1,
      layoutMode: "flex",
    },
  },
  {
    schemaVersion: 1,
    targetId: "target-region-001",
    captureRevisionId: "revision-accepted-001",
    captureSource: "accepted",
    pagePath: "index.html",
    kind: "region",
    label: "Hero content gap",
    viewport: targetViewportV1,
    document: targetDocumentV1,
    geometry: targetGeometryV1,
    regionAnchor: {
      containingBlock: { ...elementAnchorV1, tagName: "SECTION" },
      previousVisibleSibling: elementAnchorV1,
      nextVisibleSibling: { ...elementAnchorV1, tagName: "P" },
      layoutMode: "grid",
    },
  },
] satisfies TargetV1[];

const [
  pageTargetV1,
  blockTargetV1,
  elementTargetV1,
  textTargetV1,
  pointTargetV1,
  regionTargetV1,
] = targetV1Fixtures as [
  Extract<TargetV1, { kind: "page" }>,
  Extract<TargetV1, { kind: "block" }>,
  Extract<TargetV1, { kind: "element" }>,
  Extract<TargetV1, { kind: "text" }>,
  Extract<TargetV1, { kind: "point" }>,
  Extract<TargetV1, { kind: "region" }>,
];

const resolverCandidateV1: TargetResolverCandidateV1 = {
  candidateId: "candidate-hero-heading",
  score: 0.94,
  reasons: ["unique_id", "semantic_fingerprint", "geometry"],
  summary: "H1 — Build a focused landing page",
  elementAnchor: elementAnchorV1,
  geometry: targetGeometryV1,
};

const targetResolverResultsV1 = [
  {
    schemaVersion: 1,
    resolverVersion: 1,
    targetId: "target-element-001",
    captureRevisionId: "revision-accepted-001",
    resolvedRevisionId: "revision-accepted-002",
    status: "resolved",
    selectedCandidateId: resolverCandidateV1.candidateId,
    candidates: [resolverCandidateV1],
  },
  {
    schemaVersion: 1,
    resolverVersion: 1,
    targetId: "target-element-001",
    captureRevisionId: "revision-accepted-001",
    resolvedRevisionId: "revision-accepted-002",
    status: "ambiguous",
    candidates: [
      resolverCandidateV1,
      {
        ...resolverCandidateV1,
        candidateId: "candidate-secondary-heading",
        score: 0.82,
        reasons: ["semantic_fingerprint", "dom_path"],
        summary: "Secondary H1 candidate",
      },
    ],
  },
  {
    schemaVersion: 1,
    resolverVersion: 1,
    targetId: "target-element-001",
    captureRevisionId: "revision-accepted-001",
    resolvedRevisionId: "revision-accepted-002",
    status: "detached",
    candidates: [],
  },
] satisfies TargetResolverResultV1[];

const [resolvedTargetV1, ambiguousTargetV1, detachedTargetV1] =
  targetResolverResultsV1 as [
    TargetResolverResultV1 & { status: "resolved" },
    TargetResolverResultV1 & { status: "ambiguous" },
    TargetResolverResultV1 & { status: "detached" },
  ];

const bootstrap = bootstrapFixture();
const project = projectResponseFixture();
const projects = projectsResponseFixture();
const importPreview = importPreviewResponseFixture;
const proposal = proposalResponseFixture;
const createContextRequestV1 = {
  schemaVersion: "1",
  revisionId: "revision-accepted-001",
  targetId: "target-001",
  resolutionId: "resolution-001",
  attemptId: "attempt-001",
  providerId: "fake",
  requestedModel: "deterministic-v1",
  instruction: "見出しを力強くしてください",
} as const;
const attemptStatusV1 = {
  schemaVersion: "1",
  attemptId: "attempt-001",
  status: "proposal_ready",
} as const;
const providerStreamEventV1 = {
  schemaVersion: "1",
  attemptId: "attempt-001",
  sequence: 1,
  event: "text_delta",
  text: "変更案を生成しています。",
} as const;
const providerResultV1 = {
  schemaVersion: "1",
  attemptId: "attempt-001",
  attribution: proposal.proposal.attribution,
  output: { kind: "change_set", changeSet: allOperationsChangeSetFixture },
} as const;
const providerErrorV1 = {
  schemaVersion: "1",
  attemptId: "attempt-001",
  code: "timeout",
  message: "The provider timed out.",
  retryable: true,
} as const;

const extraFieldCases: ReadonlyArray<
  readonly [name: string, guard: Guard, value: unknown]
> = [
  ["bootstrap root", isBootstrapResponse, { ...bootstrap, extra: true }],
  [
    "bootstrap session",
    isBootstrapResponse,
    { ...bootstrap, session: { ...bootstrap.session, extra: true } },
  ],
  [
    "bootstrap capabilities",
    isBootstrapResponse,
    {
      ...bootstrap,
      capabilities: { ...bootstrap.capabilities, extra: true },
    },
  ],
  [
    "bootstrap import limits",
    isBootstrapResponse,
    {
      ...bootstrap,
      capabilities: {
        ...bootstrap.capabilities,
        limits: { ...bootstrap.capabilities.limits, extra: true },
      },
    },
  ],
  [
    "bootstrap AI provider",
    isBootstrapResponse,
    {
      ...bootstrap,
      capabilities: {
        ...bootstrap.capabilities,
        aiProviders: [
          { ...bootstrap.capabilities.aiProviders[0]!, extra: true },
        ],
      },
    },
  ],
  [
    "bootstrap AI provider model",
    isBootstrapResponse,
    {
      ...bootstrap,
      capabilities: {
        ...bootstrap.capabilities,
        aiProviders: [
          {
            ...bootstrap.capabilities.aiProviders[0]!,
            models: [
              {
                ...bootstrap.capabilities.aiProviders[0]!.models[0]!,
                extra: true,
              },
            ],
          },
        ],
      },
    },
  ],
  ["project response root", isProjectResponse, { ...project, extra: true }],
  ["projects response root", isProjectsResponse, { ...projects, extra: true }],
  [
    "projects array item",
    isProjectsResponse,
    {
      ...projects,
      projects: [{ ...projects.projects[0]!, extra: true }],
    },
  ],
  [
    "project",
    isProjectResponse,
    { ...project, project: { ...project.project, extra: true } },
  ],
  [
    "project file array item",
    isProjectResponse,
    {
      ...project,
      project: {
        ...project.project,
        files: [{ ...project.project.files[0]!, extra: true }],
      },
    },
  ],
  [
    "import preview response root",
    isImportPreviewResponse,
    { ...importPreview, extra: true },
  ],
  [
    "import preview",
    isImportPreviewResponse,
    {
      ...importPreview,
      importPreview: { ...importPreview.importPreview, extra: true },
    },
  ],
  [
    "import preview included item",
    isImportPreviewResponse,
    {
      ...importPreview,
      importPreview: {
        ...importPreview.importPreview,
        included: [
          { ...importPreview.importPreview.included[0]!, extra: true },
        ],
      },
    },
  ],
  [
    "import preview excluded item",
    isImportPreviewResponse,
    {
      ...importPreview,
      importPreview: {
        ...importPreview.importPreview,
        excluded: [
          { ...importPreview.importPreview.excluded[0]!, extra: true },
        ],
      },
    },
  ],
  [
    "target response root",
    isTargetResponse,
    { ...targetResponseFixture, extra: true },
  ],
  [
    "target",
    isTargetResponse,
    {
      ...targetResponseFixture,
      target: { ...targetResponseFixture.target, extra: true },
    },
  ],
  ["target v1 root", isTargetV1, { ...elementTargetV1, extra: true }],
  [
    "target v1 viewport",
    isTargetV1,
    {
      ...elementTargetV1,
      viewport: { ...elementTargetV1.viewport, extra: true },
    },
  ],
  [
    "target v1 document",
    isTargetV1,
    {
      ...elementTargetV1,
      document: { ...elementTargetV1.document, extra: true },
    },
  ],
  [
    "target v1 geometry",
    isTargetV1,
    {
      ...elementTargetV1,
      geometry: { ...elementTargetV1.geometry, extra: true },
    },
  ],
  [
    "target v1 geometry rect",
    isTargetV1,
    {
      ...elementTargetV1,
      geometry: {
        ...elementTargetV1.geometry,
        documentCssPixelRect: {
          ...elementTargetV1.geometry?.documentCssPixelRect,
          extra: true,
        },
      },
    },
  ],
  [
    "target v1 element anchor",
    isTargetV1,
    {
      ...elementTargetV1,
      elementAnchor: { ...elementTargetV1.elementAnchor, extra: true },
    },
  ],
  [
    "target v1 block metadata",
    isTargetV1,
    {
      ...blockTargetV1,
      block: { ...blockTargetV1.block, extra: true },
    },
  ],
  [
    "target v1 text anchor",
    isTargetV1,
    {
      ...textTargetV1,
      textAnchor: { ...textTargetV1.textAnchor, extra: true },
    },
  ],
  [
    "target v1 region anchor",
    isTargetV1,
    {
      ...regionTargetV1,
      regionAnchor: { ...regionTargetV1.regionAnchor, extra: true },
    },
  ],
  [
    "target v1 point",
    isTargetV1,
    {
      ...pointTargetV1,
      point: { ...pointTargetV1.point, extra: true },
    },
  ],
  [
    "target v1 point coordinates",
    isTargetV1,
    {
      ...pointTargetV1,
      point: {
        ...pointTargetV1.point,
        documentCssPixel: {
          ...pointTargetV1.point.documentCssPixel,
          extra: true,
        },
      },
    },
  ],
  [
    "target resolver root",
    isTargetResolverResultV1,
    { ...resolvedTargetV1, extra: true },
  ],
  [
    "target resolver candidate",
    isTargetResolverResultV1,
    {
      ...resolvedTargetV1,
      candidates: [{ ...resolverCandidateV1, extra: true }],
    },
  ],
  [
    "context response root",
    isContextResponse,
    { ...contextResponseFixture, extra: true },
  ],
  [
    "create context request",
    isCreateContextRequest,
    { ...createContextRequestV1, extra: true },
  ],
  [
    "context",
    isContextResponse,
    {
      ...contextResponseFixture,
      context: { ...contextResponseFixture.context, extra: true },
    },
  ],
  [
    "context provider binding",
    isContextResponse,
    {
      ...contextResponseFixture,
      context: {
        ...contextResponseFixture.context,
        provider: { ...contextResponseFixture.context.provider, extra: true },
      },
    },
  ],
  [
    "context manifest",
    isContextResponse,
    {
      ...contextResponseFixture,
      context: {
        ...contextResponseFixture.context,
        manifest: { ...contextResponseFixture.context.manifest, extra: true },
      },
    },
  ],
  [
    "context manifest entry",
    isContextResponse,
    {
      ...contextResponseFixture,
      context: {
        ...contextResponseFixture.context,
        manifest: {
          ...contextResponseFixture.context.manifest,
          entries: [
            {
              ...contextResponseFixture.context.manifest.entries[0]!,
              extra: true,
            },
          ],
          totalIncludedBytes:
            contextResponseFixture.context.manifest.entries[0]!
              .includedByteLength,
          estimatedTokens:
            contextResponseFixture.context.manifest.entries[0]!.estimatedTokens,
        },
      },
    },
  ],
  ["proposal response root", isProposalResponse, { ...proposal, extra: true }],
  [
    "proposal",
    isProposalResponse,
    { ...proposal, proposal: { ...proposal.proposal, extra: true } },
  ],
  [
    "proposal change array item",
    isProposalResponse,
    {
      ...proposal,
      proposal: {
        ...proposal.proposal,
        changes: [{ ...proposal.proposal.changes[0]!, extra: true }],
      },
    },
  ],
  [
    "proposal ChangeSet",
    isProposalResponse,
    {
      ...proposal,
      proposal: {
        ...proposal.proposal,
        changeSet: { ...proposal.proposal.changeSet, extra: true },
      },
    },
  ],
  [
    "proposal ChangeSet operation",
    isProposalResponse,
    {
      ...proposal,
      proposal: {
        ...proposal.proposal,
        changeSet: {
          ...proposal.proposal.changeSet,
          operations: [
            { ...proposal.proposal.changeSet.operations[0]!, extra: true },
          ],
        },
      },
    },
  ],
  [
    "proposal provider attribution",
    isProposalResponse,
    {
      ...proposal,
      proposal: {
        ...proposal.proposal,
        attribution: { ...proposal.proposal.attribution, extra: true },
      },
    },
  ],
  [
    "proposal provider usage",
    isProposalResponse,
    {
      ...proposal,
      proposal: {
        ...proposal.proposal,
        attribution: {
          ...proposal.proposal.attribution,
          usage: { ...proposal.proposal.attribution.usage, extra: true },
        },
      },
    },
  ],
  [
    "proposal validation",
    isProposalResponse,
    {
      ...proposal,
      proposal: {
        ...proposal.proposal,
        validation: { ...proposal.proposal.validation, extra: true },
      },
    },
  ],
  [
    "proposal validation check array item",
    isProposalResponse,
    {
      ...proposal,
      proposal: {
        ...proposal.proposal,
        validation: {
          ...proposal.proposal.validation,
          checks: [{ ...proposal.proposal.validation.checks[0]!, extra: true }],
        },
      },
    },
  ],
  [
    "approval response root",
    isApprovalResponse,
    { ...approvalResponseFixture, extra: true },
  ],
  [
    "approval",
    isApprovalResponse,
    {
      ...approvalResponseFixture,
      approval: { ...approvalResponseFixture.approval, extra: true },
    },
  ],
  [
    "decision response root",
    isDecisionResponse,
    { ...decisionResponseFixture, extra: true },
  ],
  [
    "decision",
    isDecisionResponse,
    {
      ...decisionResponseFixture,
      decision: { ...decisionResponseFixture.decision, extra: true },
    },
  ],
  [
    "decision project",
    isDecisionResponse,
    {
      ...decisionResponseFixture,
      project: { ...decisionResponseFixture.project, extra: true },
    },
  ],
  [
    "export response root",
    isExportResponse,
    { ...exportResponseFixture, extra: true },
  ],
  [
    "export receipt",
    isExportResponse,
    {
      ...exportResponseFixture,
      export: { ...exportResponseFixture.export, extra: true },
    },
  ],
  [
    "error response root",
    isApiErrorResponse,
    { ...apiErrorResponseFixture, extra: true },
  ],
  [
    "error",
    isApiErrorResponse,
    {
      ...apiErrorResponseFixture,
      error: { ...apiErrorResponseFixture.error, extra: true },
    },
  ],
  [
    "preview selection root",
    isPreviewSelectionMessage,
    { ...previewSelection, extra: true },
  ],
  [
    "preview selection rect",
    isPreviewSelectionMessage,
    { ...previewSelection, rect: { ...previewSelection.rect, extra: true } },
  ],
  [
    "preview action root",
    isPreviewActionMessage,
    { ...previewSetMode, extra: true },
  ],
  [
    "preview diagnostic root",
    isPreviewDiagnosticMessage,
    { ...previewDiagnostic, extra: true },
  ],
];

describe("canonical API v1 schema", () => {
  const ajv = new Ajv2020({ allErrors: true, strict: true });
  ajv.addKeyword({
    keyword: "x-maxUtf8Bytes",
    type: "string",
    schemaType: "number",
    validate: (limit: number, value: string) =>
      new TextEncoder().encode(value).byteLength <= limit,
  });
  const validate = ajv.compile(apiSchema);

  it.each([
    ["bootstrap", bootstrap],
    ["project", project],
    ["projects", projects],
    ["import preview", importPreview],
    ["target", targetResponseFixture],
    ["target v1 page", pageTargetV1],
    ["target v1 block", blockTargetV1],
    ["target v1 element", elementTargetV1],
    ["target v1 text", textTargetV1],
    ["target v1 point", pointTargetV1],
    ["target v1 region", regionTargetV1],
    ["target resolver resolved", resolvedTargetV1],
    ["target resolver ambiguous", ambiguousTargetV1],
    ["target resolver detached", detachedTargetV1],
    ["create context request", createContextRequestV1],
    ["context", contextResponseFixture],
    ["all ChangeSet v1 operations", allOperationsChangeSetFixture],
    ["AI attempt status", attemptStatusV1],
    ["AI provider stream event", providerStreamEventV1],
    ["AI provider result", providerResultV1],
    ["AI provider error", providerErrorV1],
    ["proposal", proposal],
    ["approval", approvalResponseFixture],
    ["decision", decisionResponseFixture],
    ["export", exportResponseFixture],
    ["error", apiErrorResponseFixture],
    ["preview selection", previewSelection],
    ["preview diagnostic", previewDiagnostic],
    ["preview set mode", previewSetMode],
    ["preview clear selection", previewClearSelection],
  ])("validates the %s fixture with Draft 2020-12", (_name, fixture) => {
    expect(validate(fixture), JSON.stringify(validate.errors)).toBe(true);
  });

  it("rejects an unversioned public DTO", () => {
    expect(validate({ ...project, schemaVersion: "2" })).toBe(false);
  });

  it("binds proposal-captured targets to exactly one source proposal", () => {
    const {
      captureProposalId: _captureProposalId,
      ...proposalTargetWithoutProposal
    } = textTargetV1;
    const acceptedTargetWithProposal = {
      ...pageTargetV1,
      captureProposalId: "proposal-foreign",
    };

    for (const invalidTarget of [
      proposalTargetWithoutProposal,
      acceptedTargetWithProposal,
    ]) {
      expect(validate(invalidTarget)).toBe(false);
      expect(isTargetV1(invalidTarget)).toBe(false);
    }
  });

  it("accepts canonical target HTML paths through the UTF-8 byte boundary", () => {
    const validPagePaths = [
      "INDEX.HTML",
      "pages/Caf\u00e9/\u30e9\u30f3\u30c7\u30a3\u30f3\u30b0.HtM",
      "docs/a#b.html",
      `${"a".repeat(TARGET_CONTRACT_LIMITS.pagePathBytes - 5)}.html`,
      `${"\u754c".repeat(169)}.html`,
    ];

    for (const pagePath of validPagePaths) {
      const target = { ...pageTargetV1, pagePath };
      expect(new TextEncoder().encode(pagePath).byteLength).toBeLessThanOrEqual(
        TARGET_CONTRACT_LIMITS.pagePathBytes,
      );
      expect(validate(target), JSON.stringify(validate.errors)).toBe(true);
      expect(isTargetV1(target)).toBe(true);
    }
  });

  it("rejects unsafe and non-HTML target page paths", () => {
    for (const pagePath of [
      "/",
      "/index.html",
      "pages/",
      "pages//index.html",
      "pages/./index.html",
      "pages/../index.html",
      "index.html?draft=1",
      "pages\\index.html",
      "bad./index.html",
      "con/index.html",
      "C:index.html",
      "pages/line\nbreak.html",
      ".html",
      "docs/.html",
      "index.css",
    ]) {
      const target = { ...pageTargetV1, pagePath };
      expect(validate(target)).toBe(false);
      expect(isTargetV1(target)).toBe(false);
    }
  });

  it("enforces target page path UTF-8 bytes and NFC normalization", () => {
    const overAsciiLimit = `${"a".repeat(
      TARGET_CONTRACT_LIMITS.pagePathBytes - 4,
    )}.html`;
    const overMultibyteLimit = `${"\u754c".repeat(170)}.html`;

    for (const pagePath of [overAsciiLimit, overMultibyteLimit]) {
      const target = { ...pageTargetV1, pagePath };
      expect(new TextEncoder().encode(pagePath).byteLength).toBeGreaterThan(
        TARGET_CONTRACT_LIMITS.pagePathBytes,
      );
      expect(validate(target)).toBe(false);
      expect(isTargetV1(target)).toBe(false);
    }

    const nfcPagePath = "pages/Caf\u00e9.HTML";
    const nfdPagePath = nfcPagePath.normalize("NFD");
    expect(validate({ ...pageTargetV1, pagePath: nfcPagePath })).toBe(true);
    expect(isTargetV1({ ...pageTargetV1, pagePath: nfcPagePath })).toBe(true);
    expect(nfdPagePath).not.toBe(nfcPagePath);
    expect(isTargetV1({ ...pageTargetV1, pagePath: nfdPagePath })).toBe(false);
  });

  it("enforces the required and exclusive fields for all target kinds", () => {
    const { elementAnchor: _elementAnchor, ...elementWithoutAnchor } =
      elementTargetV1;
    const { block: _block, ...blockWithoutMetadata } = blockTargetV1;
    const { textAnchor: _textAnchor, ...textWithoutQuote } = textTargetV1;
    const {
      containingBlock: _containingBlock,
      ...pointContextWithoutContainingBlock
    } = pointTargetV1.regionAnchor;

    const invalidTargets = [
      { ...pageTargetV1, geometry: targetGeometryV1 },
      elementWithoutAnchor,
      blockWithoutMetadata,
      textWithoutQuote,
      {
        ...pointTargetV1,
        regionAnchor: pointContextWithoutContainingBlock,
      },
      {
        ...regionTargetV1,
        geometry: {
          ...regionTargetV1.geometry,
          documentCssPixelRect: {
            ...regionTargetV1.geometry.documentCssPixelRect,
            width: 0,
          },
        },
      },
    ];

    for (const invalidTarget of invalidTargets) {
      expect(validate(invalidTarget)).toBe(false);
      expect(isTargetV1(invalidTarget)).toBe(false);
    }
  });

  it("bounds persisted target evidence and coordinate metadata", () => {
    const invalidTargets = [
      {
        ...pageTargetV1,
        label: "x".repeat(TARGET_CONTRACT_LIMITS.labelLength + 1),
      },
      {
        ...elementTargetV1,
        viewport: { ...elementTargetV1.viewport, cssWidth: 0 },
      },
      {
        ...elementTargetV1,
        elementAnchor: {
          ...elementTargetV1.elementAnchor,
          accessibleName: "x".repeat(
            TARGET_CONTRACT_LIMITS.accessibleNameLength + 1,
          ),
        },
      },
      {
        ...elementTargetV1,
        elementAnchor: {
          ...elementTargetV1.elementAnchor,
          classTokens: Array.from(
            { length: TARGET_CONTRACT_LIMITS.classTokens + 1 },
            (_value, index) => `class-${index}`,
          ),
        },
      },
      {
        ...textTargetV1,
        textAnchor: {
          ...textTargetV1.textAnchor,
          exact: "x".repeat(TARGET_CONTRACT_LIMITS.textQuoteLength + 1),
        },
      },
      {
        ...regionTargetV1,
        geometry: {
          ...regionTargetV1.geometry,
          viewportNormalizedRect: {
            ...regionTargetV1.geometry.viewportNormalizedRect,
            x: 1.01,
          },
        },
      },
    ];

    for (const invalidTarget of invalidTargets) {
      expect(validate(invalidTarget)).toBe(false);
      expect(isTargetV1(invalidTarget)).toBe(false);
    }
  });

  it("never admits a render-local runtime node handle to the wire contract", () => {
    const withRuntimeNodeHandle = {
      ...elementTargetV1,
      elementAnchor: {
        ...elementTargetV1.elementAnchor,
        runtimeNodeHandle: "preview-render-only-node-42",
      },
    };
    expect(validate(withRuntimeNodeHandle)).toBe(false);
    expect(isTargetV1(withRuntimeNodeHandle)).toBe(false);
  });

  it("enforces fail-closed resolver status envelopes", () => {
    const {
      selectedCandidateId: _selectedCandidateId,
      ...resolvedWithoutSelection
    } = resolvedTargetV1;
    const invalidResults = [
      resolvedWithoutSelection,
      {
        ...ambiguousTargetV1,
        selectedCandidateId: resolverCandidateV1.candidateId,
      },
      { ...ambiguousTargetV1, candidates: [] },
      {
        ...detachedTargetV1,
        selectedCandidateId: resolverCandidateV1.candidateId,
      },
      {
        ...resolvedTargetV1,
        candidates: Array.from(
          { length: TARGET_CONTRACT_LIMITS.resolverCandidates + 1 },
          (_value, index) => ({
            ...resolverCandidateV1,
            candidateId: `candidate-${index}`,
          }),
        ),
      },
      {
        ...resolvedTargetV1,
        candidates: [{ ...resolverCandidateV1, score: 1.01 }],
      },
      {
        ...resolvedTargetV1,
        candidates: [
          {
            ...resolverCandidateV1,
            reasons: ["geometry", "geometry"],
          },
        ],
      },
    ];

    for (const invalidResult of invalidResults) {
      expect(validate(invalidResult)).toBe(false);
      expect(isTargetResolverResultV1(invalidResult)).toBe(false);
    }
  });

  it("accepts all four strict ChangeSet v1 operations", () => {
    expect(validate(allOperationsChangeSetFixture)).toBe(true);
    expect(isChangeSetV1(allOperationsChangeSetFixture)).toBe(true);
    expect(
      allOperationsChangeSetFixture.operations.map(({ op }) => op),
    ).toEqual(["replace_text", "create_text", "rename", "delete"]);
  });

  it("rejects unknown or decorated ChangeSet operations", () => {
    const unknownOperation = {
      ...allOperationsChangeSetFixture,
      operations: [{ op: "patch", path: "index.html", content: "unsafe" }],
    };
    const decoratedOperation = {
      ...allOperationsChangeSetFixture,
      operations: [
        { ...allOperationsChangeSetFixture.operations[0]!, shell: "echo no" },
      ],
    };
    for (const invalid of [unknownOperation, decoratedOperation]) {
      expect(validate(invalid)).toBe(false);
      expect(isChangeSetV1(invalid)).toBe(false);
    }
  });

  it("rejects malformed provider attribution", () => {
    const attribution = proposal.proposal.attribution;
    const malformed = [
      { ...attribution, reportedModel: "" },
      { ...attribution, providerRequestId: "" },
      { ...attribution, usage: { totalTokens: -1 } },
      { ...attribution, usage: { totalTokens: 1, credential: "secret" } },
    ];
    for (const invalidAttribution of malformed) {
      const invalidProposal = {
        ...proposal,
        proposal: { ...proposal.proposal, attribution: invalidAttribution },
      };
      expect(validate(invalidProposal)).toBe(false);
      expect(isAiProviderAttributionV1(invalidAttribution)).toBe(false);
      expect(isProposalResponse(invalidProposal)).toBe(false);
    }
  });

  it("rejects malformed or internally inconsistent context manifests", () => {
    const manifest = contextResponseFixture.context.manifest;
    const firstEntry = manifest.entries[0]!;
    const malformed = [
      { ...manifest, totalIncludedBytes: manifest.totalIncludedBytes + 1 },
      {
        ...manifest,
        entries: [
          firstEntry,
          { ...manifest.entries[1]!, path: firstEntry.path },
        ],
      },
      {
        ...manifest,
        entries: [{ ...firstEntry, endLine: 0 }],
        totalIncludedBytes: firstEntry.includedByteLength,
        estimatedTokens: firstEntry.estimatedTokens,
      },
      {
        ...manifest,
        entries: [{ ...firstEntry, redacted: false, redactions: ["secret"] }],
        totalIncludedBytes: firstEntry.includedByteLength,
        estimatedTokens: firstEntry.estimatedTokens,
      },
    ];
    for (const invalidManifest of malformed) {
      expect(isContextManifestV1(invalidManifest)).toBe(false);
      expect(
        isContextResponse({
          ...contextResponseFixture,
          context: {
            ...contextResponseFixture.context,
            manifest: invalidManifest,
          },
        }),
      ).toBe(false);
    }
    expect(
      validate({
        ...contextResponseFixture,
        context: {
          ...contextResponseFixture.context,
          manifest: malformed[3],
        },
      }),
    ).toBe(false);
  });

  it.each(extraFieldCases)(
    "rejects an extra field in %s",
    (_name, _guard, value) => {
      expect(validate(value)).toBe(false);
    },
  );

  it("requires snapshot and base revision bindings on preview messages", () => {
    const { snapshotId: _snapshotId, ...withoutSnapshot } = previewSelection;
    expect(validate(withoutSnapshot)).toBe(false);
    expect(isPreviewSelectionMessage(withoutSnapshot)).toBe(false);
    expect(isPreviewSelectionMessage(previewSelection)).toBe(true);
    expect(isPreviewDiagnosticMessage(previewDiagnostic)).toBe(true);
  });

  it("binds mode presence exactly to the preview action", () => {
    const { mode: _mode, ...setModeWithoutMode } = previewSetMode;
    expect(validate(setModeWithoutMode)).toBe(false);
    expect(isPreviewActionMessage(setModeWithoutMode)).toBe(false);
    expect(validate({ ...previewClearSelection, mode: "select" })).toBe(false);
    expect(
      isPreviewActionMessage({ ...previewClearSelection, mode: "select" }),
    ).toBe(false);
  });

  it("rejects zero import limits and non-canonical paths in Draft 2020-12", () => {
    expect(
      validate({
        ...bootstrap,
        capabilities: {
          ...bootstrap.capabilities,
          limits: { ...bootstrap.capabilities.limits, maxFiles: 0 },
        },
      }),
    ).toBe(false);
    for (const path of [
      "assets\\private.css",
      "assets//private.css",
      "assets/",
      "./assets.css",
      "assets/./private.css",
    ]) {
      expect(
        validate({
          ...importPreview,
          importPreview: {
            ...importPreview.importPreview,
            included: [
              {
                ...importPreview.importPreview.included[0]!,
                path,
              },
            ],
          },
        }),
      ).toBe(false);
    }
  });

  it("rejects foreign preview scopes and non-canonical scoped URLs in the schema", () => {
    for (const previewOrigin of [
      "https://localhost:4174",
      "http://localhost",
      "http://child.localhost:4174",
    ]) {
      expect(validate({ ...bootstrap, previewOrigin })).toBe(false);
    }
    for (const previewUrl of [
      "http://pv-11111111111111111111111111111111.localhost.attacker.test:4174/preview/project-001/revision-001/",
      "http://user@pv-11111111111111111111111111111111.localhost:4174/preview/project-001/revision-001/",
      "http://pv-11111111111111111111111111111111.localhost:4174/preview/project-001/../secret/",
      "http://pv-11111111111111111111111111111111.localhost:4174/preview/project-001/revision-001",
      "http://pv-11111111111111111111111111111111.localhost:4174/preview/project-001/revision-001/assets/",
    ]) {
      expect(
        validate({
          ...project,
          project: { ...project.project, previewUrl },
        }),
      ).toBe(false);
    }
  });
});

describe("runtime response and bridge guards", () => {
  it("accepts every contract fixture", () => {
    expect(isBootstrapResponse(bootstrap)).toBe(true);
    expect(isProjectResponse(project)).toBe(true);
    expect(isProjectsResponse(projects)).toBe(true);
    expect(isImportPreviewResponse(importPreview)).toBe(true);
    expect(isTargetResponse(targetResponseFixture)).toBe(true);
    for (const target of targetV1Fixtures) {
      expect(isTargetV1(target)).toBe(true);
    }
    for (const result of targetResolverResultsV1) {
      expect(isTargetResolverResultV1(result)).toBe(true);
    }
    expect(isContextResponse(contextResponseFixture)).toBe(true);
    expect(isCreateContextRequest(createContextRequestV1)).toBe(true);
    expect(isChangeSetV1(allOperationsChangeSetFixture)).toBe(true);
    expect(isAiAttemptStatusV1(attemptStatusV1)).toBe(true);
    expect(isAiProviderStreamEventV1(providerStreamEventV1)).toBe(true);
    expect(isAiProviderResultV1(providerResultV1)).toBe(true);
    expect(isAiProviderErrorV1(providerErrorV1)).toBe(true);
    expect(isProposalResponse(proposal)).toBe(true);
    expect(isApprovalResponse(approvalResponseFixture)).toBe(true);
    expect(isDecisionResponse(decisionResponseFixture)).toBe(true);
    expect(isExportResponse(exportResponseFixture)).toBe(true);
    expect(isApiErrorResponse(apiErrorResponseFixture)).toBe(true);
    expect(isPreviewSelectionMessage(previewSelection)).toBe(true);
    expect(isPreviewDiagnosticMessage(previewDiagnostic)).toBe(true);
    expect(isPreviewActionMessage(previewSetMode)).toBe(true);
    expect(isPreviewActionMessage(previewClearSelection)).toBe(true);
  });

  it.each(extraFieldCases)(
    "fails closed on an extra field in %s",
    (_name, guard, value) => {
      expect(guard(value)).toBe(false);
    },
  );

  it("fails closed on sparse or decorated nested arrays", () => {
    const decoratedFiles = [...project.project.files];
    Object.assign(decoratedFiles, { extra: true });
    expect(
      isProjectResponse({
        ...project,
        project: { ...project.project, files: decoratedFiles },
      }),
    ).toBe(false);

    const sparseChanges = new Array(proposal.proposal.changes.length);
    expect(
      isProposalResponse({
        ...proposal,
        proposal: { ...proposal.proposal, changes: sparseChanges },
      }),
    ).toBe(false);

    const decoratedClassTokens = [...elementAnchorV1.classTokens];
    Object.assign(decoratedClassTokens, { extra: true });
    expect(
      isTargetV1({
        ...elementTargetV1,
        elementAnchor: {
          ...elementTargetV1.elementAnchor,
          classTokens: decoratedClassTokens,
        },
      }),
    ).toBe(false);

    const sparseReasons = new Array(resolverCandidateV1.reasons.length);
    expect(
      isTargetResolverResultV1({
        ...resolvedTargetV1,
        candidates: [{ ...resolverCandidateV1, reasons: sparseReasons }],
      }),
    ).toBe(false);

    const decoratedCandidates = [...resolvedTargetV1.candidates];
    Object.assign(decoratedCandidates, { extra: true });
    expect(
      isTargetResolverResultV1({
        ...resolvedTargetV1,
        candidates: decoratedCandidates,
      }),
    ).toBe(false);
  });

  it("rejects malformed text offsets and normalized geometry", () => {
    expect(
      isTargetV1({
        ...textTargetV1,
        textAnchor: {
          ...textTargetV1.textAnchor,
          startOffset: 30,
          endOffset: 20,
        },
      }),
    ).toBe(false);

    const { endOffset: _endOffset, ...halfBoundTextAnchor } =
      textTargetV1.textAnchor;
    expect(
      isTargetV1({ ...textTargetV1, textAnchor: halfBoundTextAnchor }),
    ).toBe(false);

    expect(
      isTargetV1({
        ...textTargetV1,
        textAnchor: {
          ...textTargetV1.textAnchor,
          endOffset: textTargetV1.textAnchor.endOffset! + 1,
        },
      }),
    ).toBe(false);

    expect(
      isTargetV1({
        ...elementTargetV1,
        geometry: {
          ...elementTargetV1.geometry!,
          viewportNormalizedRect: {
            x: 0.8,
            y: 0.2,
            width: 0.3,
            height: 0.4,
          },
        },
      }),
    ).toBe(false);
    expect(
      isTargetV1({
        ...pageTargetV1,
        viewport: { ...pageTargetV1.viewport, previewScale: Number.NaN },
      }),
    ).toBe(false);
  });

  it("treats text offsets as UTF-16 code units", () => {
    const emojiExact = "A😀B";
    expect(
      isTargetV1({
        ...textTargetV1,
        textAnchor: {
          exact: emojiExact,
          startOffset: 10,
          endOffset: 10 + emojiExact.length,
        },
      }),
    ).toBe(true);
    expect(
      isTargetV1({
        ...textTargetV1,
        textAnchor: {
          exact: emojiExact,
          startOffset: 10,
          endOffset: 13,
        },
      }),
    ).toBe(false);
  });

  it("binds resolved candidates by unique candidate id", () => {
    expect(
      isTargetResolverResultV1({
        ...resolvedTargetV1,
        selectedCandidateId: "candidate-not-present",
      }),
    ).toBe(false);
    expect(
      isTargetResolverResultV1({
        ...ambiguousTargetV1,
        candidates: [
          resolverCandidateV1,
          {
            ...resolverCandidateV1,
            score: 0.7,
            summary: "Same id, different candidate evidence",
          },
        ],
      }),
    ).toBe(false);
  });

  it("fails closed on version, hash, and attribution drift", () => {
    expect(isProjectResponse({ ...project, schemaVersion: "2" })).toBe(false);
    expect(
      isProjectResponse({
        ...project,
        project: {
          ...project.project,
          acceptedManifestSha256: "not-a-digest",
        },
      }),
    ).toBe(false);
    expect(
      isProposalResponse({
        ...proposal,
        proposal: { ...proposal.proposal, executionVerified: true },
      }),
    ).toBe(false);
  });

  it("rejects absolute, traversal, and backslash import paths", () => {
    for (const path of [
      "/srv/private/index.html",
      "../index.html",
      "assets\\secret.css",
      "C:\\private\\index.html",
      "assets//secret.css",
      "assets/",
      "assets/./secret.css",
    ]) {
      expect(
        isImportPreviewResponse({
          ...importPreview,
          importPreview: {
            ...importPreview.importPreview,
            included: [{ ...importPreview.importPreview.included[0]!, path }],
          },
        }),
      ).toBe(false);
    }
  });

  it("requires every advertised import limit to be positive", () => {
    for (const key of Object.keys(bootstrap.capabilities.limits)) {
      expect(
        isBootstrapResponse({
          ...bootstrap,
          capabilities: {
            ...bootstrap.capabilities,
            limits: { ...bootstrap.capabilities.limits, [key]: 0 },
          },
        }),
      ).toBe(false);
    }
  });

  it("accepts only the local scope base and canonical scoped preview URLs", () => {
    const scopeBase = "http://localhost:4174";
    const scopedOrigin =
      "http://pv-0123456789abcdef0123456789abcdef.localhost:4174";
    expect(isPreviewScopeBaseOrigin(scopeBase)).toBe(true);
    expect(
      isScopedPreviewUrl(
        `${scopedOrigin}/preview/project-001/revision-001/`,
        scopeBase,
      ),
    ).toBe(true);
    expect(
      isScopedPreviewUrl(
        `${scopedOrigin}/preview/project-001/revision-001/assets/app.css`,
        scopeBase,
      ),
    ).toBe(true);

    for (const invalidBase of [
      "https://localhost:4174",
      "http://127.0.0.1:4174",
      "http://localhost",
      "http://localhost:80",
      "http://localhost:4174/path",
      "http://attacker.localhost:4174",
    ]) {
      expect(isPreviewScopeBaseOrigin(invalidBase)).toBe(false);
    }

    for (const invalidUrl of [
      "http://pv-0123456789abcdef0123456789abcdef.localhost.attacker.test:4174/preview/project-001/revision-001/",
      "http://evil.pv-0123456789abcdef0123456789abcdef.localhost:4174/preview/project-001/revision-001/",
      `${scopedOrigin.replace(":4174", ":4175")}/preview/project-001/revision-001/`,
      "http://user@pv-0123456789abcdef0123456789abcdef.localhost:4174/preview/project-001/revision-001/",
      `${scopedOrigin}/preview/project-001/../secret/`,
      `${scopedOrigin}/preview/project-001/%2e%2e/secret.css`,
      `${scopedOrigin}/preview/project-001/revision-001//app.css`,
      `${scopedOrigin}/preview/project-001/revision-001`,
      `${scopedOrigin}/preview/project-001/revision-001/assets/`,
      `${scopedOrigin}/preview/project-001/revision-001/?query=secret`,
      `${scopedOrigin}/preview/project-001/revision-001/#fragment`,
    ]) {
      expect(isScopedPreviewUrl(invalidUrl, scopeBase)).toBe(false);
    }
  });

  it("accepts only the fixed privacy-safe diagnostic vocabulary", () => {
    expect(isPreviewDiagnosticMessage(previewDiagnostic)).toBe(true);
    expect(
      isPreviewDiagnosticMessage({
        ...previewDiagnostic,
        message: "file:///private/site/index.html",
      }),
    ).toBe(false);
    expect(
      isPreviewDiagnosticMessage({
        ...previewDiagnostic,
        sourceUnavailable: false,
      }),
    ).toBe(false);
    expect(
      isPreviewDiagnosticMessage({
        ...previewDiagnostic,
        code: "raw_exception",
      }),
    ).toBe(false);
  });
});
