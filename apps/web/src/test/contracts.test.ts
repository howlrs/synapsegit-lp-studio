import Ajv2020 from "ajv/dist/2020.js";
import {
  isApiErrorResponse,
  isApprovalResponse,
  isBootstrapResponse,
  isContextResponse,
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
  isTargetResponse,
} from "@synapsegit-lp/contracts";
import apiSchema from "../../../../packages/contracts/schemas/api-v1.schema.json";
import {
  apiErrorResponseFixture,
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

const bootstrap = bootstrapFixture();
const project = projectResponseFixture();
const projects = projectsResponseFixture();
const importPreview = importPreviewResponseFixture;
const proposal = proposalResponseFixture;

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
  [
    "context response root",
    isContextResponse,
    { ...contextResponseFixture, extra: true },
  ],
  [
    "context",
    isContextResponse,
    {
      ...contextResponseFixture,
      context: { ...contextResponseFixture.context, extra: true },
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
    ["context", contextResponseFixture],
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
    expect(isContextResponse(contextResponseFixture)).toBe(true);
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
