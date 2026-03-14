const GITHUB_PR_PATTERN = /https:\/\/github\.com\/[^/]+\/[^/]+\/pull\/(\d+)/;
const GITLAB_MR_PATTERN = /https:\/\/gitlab\.com\/.+\/-\/merge_requests\/(\d+)/;

/**
 * Extract a displayable PR/MR label from a URL.
 * Returns null if the URL doesn't match known patterns.
 */
export function parsePrDisplay(url: string): { label: string; number: string } | null {
  const ghMatch = url.match(GITHUB_PR_PATTERN);
  if (ghMatch) return { label: `PR #${ghMatch[1]}`, number: ghMatch[1] };

  const glMatch = url.match(GITLAB_MR_PATTERN);
  if (glMatch) return { label: `MR #${glMatch[1]}`, number: glMatch[1] };

  return null;
}

/**
 * Detect a PR/MR URL in terminal output text (already ANSI-stripped).
 * Returns the last match found (most recent) or null.
 */
export function detectPrUrl(text: string): string | null {
  const pattern = /https:\/\/(?:github\.com\/[^/]+\/[^/]+\/pull\/\d+|gitlab\.com\/.+\/-\/merge_requests\/\d+)/g;
  let lastMatch: string | null = null;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(text)) !== null) {
    lastMatch = match[0];
  }
  return lastMatch;
}
