import type { PublicationDraft, Review } from "@synapsegit-lp/contracts";
import { initialStudioState, studioReducer } from "../app/studio-reducer";
import {
  HASH_A,
  HASH_B,
  projectFixture,
  proposalResponseFixture,
  targetResponseFixture,
} from "./fixtures";

const project = {
  ...projectFixture(),
  activeReview: {
    reviewId: "review-001",
    proposalId: "proposal-001",
    baseRevisionId: "revision-accepted-001",
    status: "reconciliation_required" as const,
  },
  history: [],
};

const restoredReview: Review = {
  reviewId: "review-001",
  projectId: "project-001",
  proposalId: "proposal-001",
  status: "reconciliation_required",
  reconciliationRequired: true,
  proposal: {
    ...proposalResponseFixture.proposal,
    target: targetResponseFixture.target,
    targetResolution: targetResponseFixture.resolution,
    instruction: "保存済みのHuman request",
    derivedFromProposalId: null,
  },
  decision: null,
};

const failedReview: Review = {
  ...restoredReview,
  status: "failed",
  reconciliationRequired: false,
};

const publication: PublicationDraft = {
  id: "publication-001",
  revisionId: "revision-accepted-001",
  sha256: HASH_A,
  byteLength: 512,
  files: [
    {
      path: "projection.json",
      mediaType: "application/json",
      sha256: HASH_B,
      byteLength: 3,
      utf8: "{}\n",
    },
  ],
  downloadUrl: "/api/v1/publications/publication-001/download",
  networkWrites: false,
  remotePublication: "separate_human_action",
};

describe("C7/C8 studio resume state", () => {
  it("locks a reconciliation-required project and restores its exact review context", () => {
    const loaded = studioReducer(initialStudioState, {
      type: "PROJECT_LOADED",
      project,
      origin: "opened",
    });
    expect(loaded.decisionReconciliationRequired).toBe(true);

    const restoring = studioReducer(loaded, {
      type: "OPERATION_STARTED",
      operation: "restoring_review",
    });
    const restored = studioReducer(restoring, {
      type: "REVIEW_RESTORED",
      review: restoredReview,
    });
    expect(restored.proposal?.id).toBe("proposal-001");
    expect(restored.proposalTarget).toEqual(targetResponseFixture.target);
    expect(restored.proposalInstruction).toBe("保存済みのHuman request");
    expect(restored.previewSource).toBe("proposed");
    expect(restored.decisionReconciliationRequired).toBe(true);
  });

  it("clears the pending proposal only after reconciliation reports a terminal receipt", () => {
    const restored = studioReducer(
      studioReducer(initialStudioState, {
        type: "PROJECT_LOADED",
        project,
        origin: "opened",
      }),
      { type: "REVIEW_RESTORED", review: restoredReview },
    );
    const terminal: Review = {
      reviewId: "review-001",
      projectId: "project-001",
      proposalId: "proposal-001",
      status: "adopted",
      reconciliationRequired: false,
      proposal: null,
      decision: {
        proposalId: "proposal-001",
        disposition: "adopted_unchanged",
        revisionId: "revision-accepted-002",
        artifactManifestSha256: HASH_B,
      },
    };
    const reconciled = studioReducer(restored, {
      type: "REVIEW_RECONCILED",
      review: terminal,
    });
    expect(reconciled.proposal).toBeNull();
    expect(reconciled.decisionReconciliationRequired).toBe(false);
    expect(reconciled.previewSource).toBe("accepted");
  });

  it("retains a terminally denied Proposal while permanently locking reconciliation and Decision", () => {
    const failedProject = {
      ...project,
      activeReview: {
        ...project.activeReview,
        status: "failed" as const,
      },
    };
    const loaded = studioReducer(initialStudioState, {
      type: "PROJECT_LOADED",
      project: failedProject,
      origin: "opened",
    });
    expect(loaded.decisionReconciliationRequired).toBe(false);
    expect(loaded.reviewTerminalFailure).toBe(true);

    const restored = studioReducer(loaded, {
      type: "REVIEW_RESTORED",
      review: failedReview,
    });
    expect(restored.proposal?.id).toBe("proposal-001");
    expect(restored.previewSource).toBe("proposed");
    expect(restored.decisionReconciliationRequired).toBe(false);
    expect(restored.reviewTerminalFailure).toBe(true);
    expect(restored.announcement).toContain("永続的に失敗");
  });

  it("retains an exact-byte publication draft separately from static export state", () => {
    const ready = studioReducer(initialStudioState, {
      type: "PUBLICATION_READY",
      publication,
    });
    expect(ready.publicationDraft).toEqual(publication);
    expect(ready.exportReceipt).toBeNull();
    expect(ready.announcement).toContain("remote write");

    const closed = studioReducer(ready, { type: "PUBLICATION_CLOSED" });
    expect(closed.publicationDraft).toBeNull();
  });
});
