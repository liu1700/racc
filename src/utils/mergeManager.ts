import type {
  MergeItemStatus,
  MergeQueueItem,
  MergeRun,
} from "../types/merge";
import type { TaskStatus } from "../types/task";

const GITHUB_PR_URL = /^https:\/\/github\.com\/[^/]+\/[^/]+\/pull\/\d+$/;

export function taskCanUseMergeManager(
  status: TaskStatus,
  prUrl: string | null | undefined,
): boolean {
  return status === "working" && Boolean(prUrl && GITHUB_PR_URL.test(prUrl));
}

export function getTaskMergeState(
  items: MergeQueueItem[],
  taskId: number,
): MergeItemStatus | null {
  return items.find((item) => item.task_id === taskId)?.status ?? null;
}

export function shipRunCanStart(
  items: MergeQueueItem[],
  activeRun: MergeRun | null,
): boolean {
  return activeRun === null && items.some((item) => item.status === "queued");
}
