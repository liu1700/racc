# Ship — Create PR, Push, Sync Wiki, Optionally Merge

This command packages up your finished work into a PR, pushes it, syncs any wiki changes to the GitHub Wiki repo, and optionally merges the PR and returns you to main.

## Workflow

### 1. Assess Current State

Run these in parallel:
- `git status` — check for uncommitted changes
- `git log --oneline origin/main..HEAD` — see commits to ship
- `git branch --show-current` — check current branch
- `git diff origin/main..HEAD --stat` — see changed files
- `git remote -v` — get repo info for wiki URL

If there are uncommitted changes, warn the user and stop. If there are no new commits vs origin/main, tell the user there's nothing to ship and stop.

### 2. Create Feature Branch (if on main)

If the current branch is `main` or `master`:
- Analyze the commits to generate a branch name (e.g., `feat/activity-panel`, `fix/auth-bug`)
- Run `git checkout -b <branch-name>`

If already on a feature branch, use it as-is.

### 3. Push the Branch

```bash
git push -u origin <branch-name>
```

### 4. Create the PR

Analyze ALL commits from `origin/main..HEAD` (not just the latest) to generate:
- **Title:** Short, under 70 characters, follows conventional commit style
- **Body:** Use this format with a HEREDOC:

```bash
gh pr create --title "<title>" --body "$(cat <<'EOF'
## Summary
<2-4 bullet points summarizing the changes>

## Test plan
<bulleted checklist of testing steps>

Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Save the PR URL from the output.

### 5. Sync Wiki (if changed)

Check if any files in `wiki/` were modified in the commits:

```bash
git diff origin/main..HEAD --name-only | grep '^wiki/'
```

If wiki files changed:
1. Derive the wiki repo URL from the main repo remote (replace `.git` with `.wiki.git`, or append `.wiki.git`)
2. Clone the wiki repo to `/tmp/wiki-sync-<timestamp>`
3. Copy the git user.name and user.email from the main repo config to the cloned wiki repo
4. Copy all changed wiki files from `wiki/` to the cloned repo root
5. Commit with a descriptive message (e.g., "docs: sync wiki — update UI Design, Feature Spec")
6. Push
7. Clean up the temp directory

If no wiki files changed, skip this step silently.

### 6. Report Results

Print a summary:
- PR URL
- Branch name
- Number of commits
- Wiki sync status (synced N files / no wiki changes)

### 7. Ask About Merge

Ask the user: **"PR created. Want me to merge it now?"**

If the user says yes:
1. Merge the PR: `gh pr merge <pr-url> --merge` (use merge commit, not squash or rebase, unless the user specifies otherwise)
2. Switch back to main: `git checkout main`
3. Pull latest: `git pull`
4. Confirm: "Merged, back on main, up to date."

If the user says no, just say "Done! PR is ready for review at <url>."

## Important

- NEVER force push or use destructive git operations
- NEVER push directly to main — always go through a PR
- If `gh` CLI is not authenticated, tell the user to run `gh auth login` first
- If the wiki repo clone fails (e.g., wiki not enabled), warn but don't fail — the PR is still valid
- Keep the PR body concise — focus on what changed and why, not implementation details
