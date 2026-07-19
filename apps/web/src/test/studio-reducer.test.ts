import { initialStudioState, studioReducer } from "../app/studio-reducer";
import {
  contextResponseFixture,
  elementTargetFixture,
  projectFixture,
  proposalResponseFixture,
  targetResolutionFixture,
  targetResponseFixture,
} from "./fixtures";

describe("studio reducer Accepted invariant", () => {
  it("keeps Decision adoption locked after an outcome-unknown failure", () => {
    const failed = studioReducer(initialStudioState, {
      type: "FAILED",
      message: "Decision outcome unknown",
      requiresDecisionReconciliation: true,
    });
    const dismissed = studioReducer(failed, { type: "DISMISS_ERROR" });

    expect(failed.decisionReconciliationRequired).toBe(true);
    expect(dismissed.decisionReconciliationRequired).toBe(true);
    expect(dismissed.error).toBeNull();
  });

  it("does not infer an Accepted revision from Decision completion", () => {
    const before = {
      ...initialStudioState,
      project: projectFixture("revision-accepted-001"),
    };
    const deciding = studioReducer(before, {
      type: "DECISION_COMMITTED",
      disposition: "adopted_unchanged",
    });

    expect(deciding.project?.revisionId).toBe("revision-accepted-001");
    expect(deciding.operation).toBe("refreshing_project");

    const refreshed = studioReducer(deciding, {
      type: "PROJECT_REFRESHED",
      project: projectFixture("revision-accepted-002"),
      afterDecision: true,
    });
    expect(refreshed.project?.revisionId).toBe("revision-accepted-002");
    expect(refreshed.operation).toBeNull();
  });

  it("detaches a target captured against an older revision", () => {
    const before = {
      ...initialStudioState,
      project: projectFixture("revision-accepted-001"),
      target: {
        target: targetResponseFixture.target,
        resolution: targetResponseFixture.resolution,
        resolutionId: targetResponseFixture.resolutionId,
      },
    };
    const refreshed = studioReducer(before, {
      type: "PROJECT_REFRESHED",
      project: projectFixture("revision-accepted-002"),
      afterDecision: true,
    });
    expect(refreshed.target).toBeNull();
  });

  it("pins the reviewed target when a Proposed preview selects another element", () => {
    const proposed = studioReducer(
      {
        ...initialStudioState,
        project: projectFixture(),
        target: {
          target: targetResponseFixture.target,
          resolution: targetResponseFixture.resolution,
          resolutionId: targetResponseFixture.resolutionId,
        },
      },
      {
        type: "PROPOSAL_READY",
        proposal: proposalResponseFixture.proposal,
        instruction: contextResponseFixture.context.instruction,
      },
    );
    const nextTarget = {
      target: {
        ...elementTargetFixture,
        targetId: "target-002",
        label: "ヒーロー説明文",
        elementAnchor: {
          ...elementTargetFixture.elementAnchor,
          uniqueElementId: "hero-copy",
        },
      },
      resolution: {
        ...targetResolutionFixture,
        targetId: "target-002",
        selectedCandidateId: "candidate-hero-copy",
        candidates: [
          {
            ...targetResolutionFixture.candidates[0]!,
            candidateId: "candidate-hero-copy",
          },
        ],
      },
      resolutionId: "resolution-002",
    };
    const selected = studioReducer(proposed, {
      type: "TARGET_SELECTED",
      target: nextTarget,
    });

    expect(selected.target).toEqual(nextTarget);
    expect(selected.proposalTarget).toEqual(targetResponseFixture.target);
  });

  it("pins the reviewed instruction when a deferred Proposal resolves", () => {
    const reviewed = {
      ...initialStudioState,
      project: projectFixture(),
      contextReview: contextResponseFixture.context,
    };
    const generating = studioReducer(reviewed, {
      type: "OPERATION_STARTED",
      operation: "generating_proposal",
    });
    const withoutDialog = studioReducer(generating, {
      type: "CONTEXT_CLOSED",
    });
    const proposed = studioReducer(withoutDialog, {
      type: "PROPOSAL_READY",
      proposal: proposalResponseFixture.proposal,
      instruction: contextResponseFixture.context.instruction,
    });

    expect(proposed.contextReview).toBeNull();
    expect(proposed.proposalInstruction).toBe(
      contextResponseFixture.context.instruction,
    );
  });

  it("invalidates a Target when the preview source changes", () => {
    const before = {
      ...initialStudioState,
      project: projectFixture(),
      target: {
        target: targetResponseFixture.target,
        resolution: targetResponseFixture.resolution,
        resolutionId: targetResponseFixture.resolutionId,
      },
    };

    const proposed = studioReducer(before, {
      type: "SET_PREVIEW_SOURCE",
      source: "proposed",
    });

    expect(proposed.previewSource).toBe("proposed");
    expect(proposed.target).toBeNull();
    expect(proposed.contextReview).toBeNull();
  });

  it("clamps custom viewport width to the supported 320–1920 range", () => {
    const narrow = studioReducer(initialStudioState, {
      type: "SET_VIEWPORT",
      preset: "custom",
      customWidth: 20,
    });
    const wide = studioReducer(narrow, {
      type: "SET_VIEWPORT",
      preset: "custom",
      customWidth: 9000,
    });
    expect(narrow.customWidth).toBe(320);
    expect(wide.customWidth).toBe(1920);
  });
});
