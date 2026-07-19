import { initialStudioState, studioReducer } from "../app/studio-reducer";
import {
  projectFixture,
  proposalResponseFixture,
  targetResponseFixture,
} from "./fixtures";

describe("studio reducer Accepted invariant", () => {
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
      target: targetResponseFixture.target,
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
        target: targetResponseFixture.target,
      },
      { type: "PROPOSAL_READY", proposal: proposalResponseFixture.proposal },
    );
    const nextTarget = {
      ...targetResponseFixture.target,
      id: "target-002",
      elementId: "hero-copy",
      label: "ヒーロー説明文",
    };
    const selected = studioReducer(proposed, {
      type: "TARGET_SELECTED",
      target: nextTarget,
    });

    expect(selected.target).toEqual(nextTarget);
    expect(selected.proposalTarget).toEqual(targetResponseFixture.target);
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
