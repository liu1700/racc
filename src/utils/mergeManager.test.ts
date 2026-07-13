import { describe, expect, test } from "bun:test";
import {
  getTaskMergeState,
  shipRunCanStart,
  taskCanUseMergeManager,
} from "./mergeManager";
import type { MergeQueueItem, MergeRun } from "../types/merge";

const item = (status: MergeQueueItem["status"]): MergeQueueItem => ({
  id: 1,
  repo_id: 1,
  task_id: 9,
  source_session_id: 3,
  pr_url: "https://github.com/acme/widgets/pull/12",
  status,
  run_id: null,
  result_message: null,
  added_at: "2026-07-12 12:00:00",
  updated_at: "2026-07-12 12:00:00",
});

describe("merge manager selectors", () => {
  test("only offers Ready to merge for a working task with an exact GitHub PR URL", () => {
    expect(taskCanUseMergeManager("working", "https://github.com/acme/widgets/pull/12")).toBe(true);
    expect(taskCanUseMergeManager("closed", "https://github.com/acme/widgets/pull/12")).toBe(false);
    expect(taskCanUseMergeManager("working", "https://gitlab.com/acme/widgets/-/merge_requests/12")).toBe(false);
    expect(taskCanUseMergeManager("working", "https://github.com/acme/widgets/pull/12/files")).toBe(false);
  });

  test("exposes a task's queue state and only enables Ship All for an idle queued manager", () => {
    const queued = item("queued");
    expect(getTaskMergeState([queued], 9)).toBe("queued");
    expect(getTaskMergeState([queued], 10)).toBeNull();
    expect(shipRunCanStart([queued], null)).toBe(true);

    const active = { status: "shipping" } as MergeRun;
    expect(shipRunCanStart([queued], active)).toBe(false);
    expect(shipRunCanStart([item("succeeded")], null)).toBe(false);
  });
});
