---
id: keeptrack
name: KEEPTRACK
description: Cross-session issue capture, user-approved recording, deferred execution, and explicit authorization before implementation.
postInstall:
  - agents
---

# KEEPTRACK — Cross-Session Issue Tracking & Handoff

This rule defines a lightweight, persistent workflow for capturing, refining, tracking, and resolving development issues across independent Coding Agent sessions.

**Core principle: Clarify before recording. Ask before executing. Preserve user control over both issue definitions and implementation decisions.**

All Coding Agents and Subagents working in the current project MUST comply with this rule.

Normative keywords:

- **MUST**: Mandatory requirement.
- **MUST NOT**: Strict prohibition.
- **SHOULD**: Recommended unless a justified exception exists.

## 1. Core Workflow

KEEPTRACK separates issue handling into two independently authorized operations:

1. **Capture and Record** — Understand, clarify, and propose an issue record. Persist it only after explicit user confirmation.
2. **Review and Execute** — Discover pending issues and propose processing. Begin investigation or implementation only after explicit user approval.

**User confirmation is mandatory at both boundaries.**

Approval to record an issue MUST NOT be interpreted as approval to investigate or fix it.

Approval to process one issue MUST NOT authorize unrelated issues or broader development work.

## 2. Persistent Issue Storage

Default location:

`PROJECT_ROOT/docs/agent-issues/`

Each issue MUST have its own Markdown file.

Recommended filename:

`YYYYMMDD-HHMMSS-<short-description>-<unique-id>.md`

Rules:

- **MUST** store confirmed issue records in a persistent project location.
- **MUST** use unique filenames to prevent concurrent session collisions.
- **MUST** preserve existing issue records and their original observations.
- **MUST NOT** store the only copy of an issue in Git-ignored `tmp/`.
- **MUST NOT** persist an unapproved issue draft.
- **SHOULD** avoid shared mutable index files.
- **SHOULD** reuse an existing authoritative issue tracker when appropriately configured.

Local Markdown files are the default. Publishing to GitHub Issues or another external tracker requires appropriate authorization.

## 3. Capture Mode — Clarify Before Recording

Activate Capture Mode when the user reports a problem and asks an Agent to record, remember, track, or defer it.

### 3.1 Understand the User's Observation

The capturing Agent MUST:

1. Understand the user's original description.
2. Identify the relevant feature, page, behavior, or component.
3. Distinguish observed symptoms from suspected causes.
4. Identify ambiguous terminology and missing details.
5. Check accessible existing issue records for obvious duplicates.
6. Prepare a proposed issue description.

The Agent MAY perform narrowly scoped, read-only repository inspection to clarify technical terminology or relevant implementation details.

The Agent MUST distinguish verified facts, user observations, assumptions, and hypotheses.

### 3.2 Improve the Description

The capturing Agent SHOULD:

- Correct inaccurate or ambiguous technical terminology.
- Rewrite unclear descriptions into precise, actionable statements.
- Identify the observed behavior and expected behavior.
- Suggest reproduction steps based on available information.
- Identify relevant components or potential impact when supported by evidence.
- Suggest an appropriate priority with a short justification.

Rules:

- **MUST** preserve the meaning of the user's original observation.
- **MUST NOT** silently replace the user's intended meaning with an Agent assumption.
- **MUST NOT** present inferred root causes as confirmed findings.
- **MUST NOT** invent reproduction results or technical evidence.
- **SHOULD** explain material corrections or assumptions concisely.
- **SHOULD** ask focused clarification questions when essential information is missing.

The Agent may recommend a corrected interpretation. The user retains final authority over the issue definition.

### 3.3 Present the Draft for Approval

Before creating or updating a persistent issue record, the Agent MUST present a proposed version containing:

- Suggested title
- Refined problem description
- Expected behavior
- Known reproduction steps
- Evidence and relevant context
- Suggested priority
- Important assumptions or uncertainties

The Agent MUST explicitly ask:

**"Does this description accurately reflect the problem? May I save it as an issue?"**

The Agent MUST wait for the user's response.

Possible outcomes:

- **Approved:** Persist the issue.
- **Revision requested:** Update the proposed description and request confirmation again.
- **Rejected or deferred:** End Capture Mode without creating the issue.

**MUST NOT** create a local issue file, remote issue, task ticket, or other persistent issue record before confirmation.

The proposed draft should remain in the conversation until approved.

### 3.4 Record the Approved Issue

After explicit user confirmation, the Agent MUST:

1. Create the persistent issue record.
2. Preserve the original observation and the approved refined description.
3. Include available reproduction steps and evidence.
4. Assign the initial status `OPEN`.
5. Confirm the saved path and issue ID to the user.

Recording an issue MUST NOT trigger investigation, implementation, or test execution.

Capture Mode MUST NOT modify application source code, tests, or runtime configuration.

## 4. Issue Record Format

Each confirmed issue SHOULD use the following format:

```markdown
---
id: ISSUE-<unique-id>
status: OPEN
priority: MEDIUM
created: <ISO-8601 timestamp>
updated: <ISO-8601 timestamp>
owner: unassigned
---

# Issue Title

## Original Observation

The user's original report.

## Approved Description

The refined description confirmed by the user.

## Expected Behavior

The intended result.

## Observed Behavior

The reported behavior.

## Reproduction Steps

Known steps, or explicitly state that they are unknown.

## Evidence

Screenshots, logs, links, or relevant files.

## Notes and Uncertainties

Verified facts, hypotheses, missing details, and assumptions.

## Resolution

Implementation details, populated only after authorized work.

## Verification

Actual tests and results, populated after execution.
```

The original observation MUST remain available even when the refined description is updated.

Material changes to the approved problem definition require renewed user confirmation.

## 5. Issue Status

| Status | Meaning |
| --- | --- |
| `OPEN` | Confirmed and recorded; awaiting execution approval |
| `IN_PROGRESS` | Explicitly approved and currently being processed |
| `BLOCKED` | Approved work cannot proceed because of a documented blocker |
| `DONE` | Resolution completed and appropriately verified |
| `WONTFIX` | Explicitly reviewed and intentionally declined |

Rules:

- **MUST** preserve accurate issue status.
- **MUST** update timestamps when changing status.
- **MUST NOT** mark an issue `IN_PROGRESS` before execution approval.
- **MUST NOT** mark an issue `DONE` without appropriate verification.
- **MUST** record reasons for `BLOCKED` and `WONTFIX`.
- **MUST** preserve investigation history and resolution evidence.

## 6. Review Mode — Ask Before Executing

When an Agent encounters pending KEEPTRACK issues, it MAY perform read-only discovery and summarize them.

Discovering an issue does not authorize processing it.

### 6.1 Discover and Present

At an appropriate task boundary, an Agent SHOULD:

1. Read accessible `OPEN` issue records.
2. Identify relevant pending issues.
3. Check current status and ownership.
4. Present a concise summary to the user.
5. Recommend a processing order when useful.
6. Ask for explicit execution approval.

Example:

> I found 3 pending issues:
>
> - ISSUE-001: User search loses its filter after pagination.
> - ISSUE-002: Avatar fails to render.
> - ISSUE-003: Logout does not update the navigation state.
>
> Would you like me to start working on these issues? I recommend starting with ISSUE-003.

### 6.2 Mandatory Execution Approval

**Before investigating, reproducing, modifying, testing, or resolving a pending issue, the Agent MUST obtain explicit user approval.**

Rules:

- **MUST** identify the issue or specific batch proposed for execution.
- **MUST** describe the intended scope at an appropriate level.
- **MUST** wait for the user's decision.
- **MUST NOT** automatically start fixing issues after reading them.
- **MUST NOT** treat `OPEN` status as execution authorization.
- **MUST NOT** treat a completed development task as implicit authorization for deferred issues.
- **MUST NOT** change issue ownership or status to `IN_PROGRESS` before approval.
- **MUST NOT** delegate an unapproved issue to a Subagent for investigation or implementation.
- **MUST NOT** interpret approval of one issue as authorization for other issues.

A clear instruction such as "Process ISSUE-001" or "Fix all currently OPEN KEEPTRACK issues" can provide execution approval for the explicitly identified scope.

Ambiguous instructions require clarification before execution.

Approval remains limited to the approved task scope and applicable project safety rules.

### 6.3 After Approval

Once explicitly authorized, the Agent SHOULD:

1. Recheck the issue's current status and ownership.
2. Claim the issue and mark it `IN_PROGRESS`.
3. Reproduce or investigate the reported behavior.
4. Identify the root cause with supporting evidence.
5. Implement the smallest correct and maintainable fix.
6. Run appropriate tests and regression checks.
7. Record the actual resolution and verification results.
8. Mark the issue `DONE` only after successful verification.

If another Agent is already processing the issue, coordinate before proceeding.

If the issue cannot be resolved, record the blocker and update its status appropriately.

Changes that materially exceed the approved scope require additional authorization.

## 7. Cross-Session Handoff

Issue records MUST contain enough context for another Agent to understand the problem without the original conversation.

Rules:

- **MUST** preserve approved descriptions and original observations.
- **MUST** preserve available reproduction information and evidence.
- **MUST** document investigation progress and unresolved blockers.
- **MUST** identify unfinished work during handoff.
- **MUST** maintain accurate ownership and status.
- **MUST** protect work from concurrent sessions.
- **MUST NOT** assume that a previous session's approval authorizes a newly initiated, unrelated execution attempt.

For separate Git Worktrees, machines, or environments, issue records must be synchronized or accessed through an explicitly shared authoritative location.

Each Agent MUST check current issue status before claiming ownership.

## 8. Integration With Existing Development Work

KEEPTRACK is an issue coordination rule, not a replacement for the active development workflow.

- **MUST** preserve the priorities of ongoing authorized development.
- **MUST NOT** interrupt active work to process newly captured issues unless requested.
- **SHOULD** check pending issues at natural task boundaries.
- **MUST** present pending issues for approval before starting their execution.
- **MUST** respect `KEEPTIDY.md` for temporary resources and Git safety.
- **MUST** respect `KEEPLEAN.md` for implementation scope and maintainability.
- **MUST** respect `KEEPSECURED.md` for authorization, security, and sensitive data.

Existing task trackers, PR workflows, and Coding Agent Skills may be used after the necessary authorization has been granted.

## 9. Completion and Delivery

Before marking an issue `DONE`, verify:

- [ ] The issue was recorded after user confirmation.
- [ ] Execution received explicit approval.
- [ ] The implemented change addresses the approved issue.
- [ ] Appropriate tests were actually executed.
- [ ] The results have been documented.
- [ ] Existing unrelated work has been protected.
- [ ] Remaining risks or limitations have been recorded.
- [ ] The final status accurately reflects the outcome.

At task completion, the Agent SHOULD report:

1. Issues completed and verified.
2. Issues still awaiting approval.
3. Blocked issues and reasons.
4. Outstanding work requiring user decisions.

**KEEPTRACK ensures that every issue is accurately described before recording and explicitly approved before execution, while preserving continuity across independent Agent sessions.**

## Summary for `AGENTS.md`

The `agents` step injects the marked region below into the project's `AGENTS.md`, so the two boundaries — record, then execute — stay visible without loading the whole rule. Keep it behavioral and short.

<!-- keeprules:agents -->
### Issue capture

- Refine and clarify the user's description.
- Show the proposed issue record.
- Wait for explicit user confirmation before saving.
- Do not implement or investigate during capture-only tasks.

### Issue execution

- Review pending issues at task boundaries.
- Present the pending issues to the user.
- ALWAYS ask for explicit approval before starting work.
- Never automatically fix an issue merely because it is OPEN.
- Update issue status only after the corresponding authorization.
<!-- /keeprules:agents -->
