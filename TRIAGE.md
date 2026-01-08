# Triage Guide

This guide explains how to triage issues and pull requests in the superseedr project using our standardized labeling system and MoSCoW-driven Kanban workflow.

---

## Overview

Triage is the process of reviewing new issues and PRs, gathering necessary information, and categorizing them appropriately. Our triage system uses:

- **Labels** to categorize and prioritize work
- **Project Board** to visualize workflow state
- **Roadmap Phases** to align with development timeline

---

## Quick Reference

### Label Categories

Every issue/PR should typically have:

1. **One Triage/State label** - Current status (`triage: new`, `triage: confirmed`, etc.)
2. **One Type label** - What kind of work (`type: bug`, `type: feature`, etc.)
3. **One Priority label** - MoSCoW priority (`priority: must`, `priority: should`, etc.)
4. **One Phase label** - Roadmap timing (`phase: 0 - pre-v1.0`, `phase: 1 - v1.x`, etc.)

**Optional labels:**
- `good first issue` - Suitable for newcomers
- `help wanted` - Need community assistance
- `discussion` - Requires team discussion
- `needs reproduction` - Cannot reproduce the issue yet
- `security` - Security-related concern

---

## Triage Process

### Step 1: Initial Review

When a new issue or PR arrives in the **Inbox** column:

1. **Read the description thoroughly**
2. **Check for completeness:**
   - Is the issue clearly described?
   - For bugs: Are there reproduction steps?
   - For features: Is the use case explained?
   - For PRs: Is there a clear description and linked issue?

### Step 2: Gather Information

If information is missing:

1. Add `triage: needs info` label
2. Move to **Needs Information** column
3. Comment asking for specific details:
   - "Could you provide the exact error message?"
   - "What version of superseedr are you using?"
   - "Can you provide steps to reproduce this?"
   - "Could you explain the use case for this feature?"

**Wait for response before proceeding.**

### Step 3: Confirm Validity

Once you have enough information:

1. **For bugs:**
   - Verify the bug exists
   - Try to reproduce it
   - If cannot reproduce: Add `needs reproduction` and move to **Needs Information**
   - If confirmed: Add `triage: confirmed` and continue

2. **For features:**
   - Evaluate if it aligns with project goals
   - Check if it duplicates existing functionality
   - If valid: Add `triage: confirmed` and continue

3. **For PRs:**
   - Check if it addresses a real need
   - Verify it follows project standards
   - Ensure tests pass
   - If needs work: Add `triage: needs info` with feedback

4. **For duplicates:**
   - Add `triage: duplicate` label
   - Comment with link to original issue
   - Close the issue
   - Move to **Won't Do** column

5. **For invalid/rejected items:**
   - Add `triage: wontfix` label
   - Explain why it won't be addressed
   - Close the issue/PR
   - Move to **Won't Do** column

### Step 4: Categorize with Type Label

Add the appropriate type label:

| Label | Use When |
|-------|----------|
| `type: bug` | Something is broken or behaving incorrectly |
| `type: feature` | Request for new functionality |
| `type: enhancement` | Improvement to existing functionality |
| `type: documentation` | Documentation changes only |
| `type: performance` | Performance or efficiency improvements |
| `type: refactor` | Internal code cleanup with no user-facing changes |
| `type: test` | Test additions or improvements |
| `type: question` | Usage question or clarification request |

### Step 5: Assign Priority (MoSCoW)

Determine the priority level:

| Priority | Use When | Examples |
|----------|----------|----------|
| `priority: must` | **Essential** for the next release. Blocking issue. | Critical bugs, security issues, release blockers |
| `priority: should` | **Important** but not critical. Should be done soon. | Important bugs, valuable features, significant improvements |
| `priority: could` | **Nice-to-have**. Add if time permits. | Minor enhancements, convenience features, polish |
| `priority: won't-do` | **Explicitly rejected**. Out of scope. | Features that don't align with project goals, duplicates |

**Priority Guidelines:**

- **Must Do:** Reserved for truly critical items. Keep this list small and focused.
- **Should Do:** The bulk of planned work. Important but can wait if needed.
- **Could Do:** Nice additions that add value but aren't essential.
- **Won't Do:** Be respectful but firm. Explain why it doesn't fit.

### Step 6: Assign Roadmap Phase

Map the work to a roadmap phase based on [ROADMAP.md](ROADMAP.md):

| Phase | Use When | Examples |
|-------|----------|----------|
| `phase: 0 - pre-v1.0` | Core stability work needed before v1.0 release | Critical bugs, core features, packaging, essential TUI work |
| `phase: 1 - v1.x` | Post-v1.0 improvements and enhancements | Selective downloading, queue management, TUI enhancements |
| `phase: 2 - v2.0+` | Major features requiring significant work | IPv6 support, daemon mode, REST API, web UI |
| `phase: future` | Good ideas without specific timeline | Experimental features, "someday" items, ideas needing discussion |

**Phase Selection Tips:**

- Check ROADMAP.md to see where similar work is planned
- `priority: must` items usually belong in `phase: 0 - pre-v1.0`
- When uncertain, use `phase: future` and discuss with the team
- Phases can be adjusted as plans evolve

### Step 7: Move to Appropriate Column

Based on priority, move the issue to the corresponding MoSCoW column:

- `priority: must` → **Must Do** column
- `priority: should` → **Should Do** column
- `priority: could` → **Could Do** column
- `priority: won't-do` → **Won't Do** column

Leave the item in **Ready for Triage** if you need team input before assigning priority.

### Step 8: Add Optional Labels

Consider adding:

- `good first issue` - For straightforward issues suitable for newcomers
- `help wanted` - When you'd welcome community contributions
- `discussion` - If the issue needs team discussion before proceeding
- `security` - For security-related issues (handle with appropriate care)

---

## Pull Request Specific Triage

PRs follow the same general process with these additions:

### PR-Specific Checks

1. **Links to issue:** Does it reference the issue it addresses?
2. **Description:** Is it clear what changes were made and why?
3. **Tests:** Are there appropriate tests?
4. **CI/CD:** Are automated checks passing?
5. **Scope:** Is it focused on one logical change?

### PR Workflow

1. **New PR arrives** → Automatically goes to **Inbox**
2. **Initial review** → Check completeness, add type/priority/phase labels
3. **Ready for review** → Move to **In Review** column
4. **Changes requested** → Add `triage: blocked`, keep in **In Review**
5. **Approved and merged** → Automatically moves to **Done**
6. **Rejected** → Add `triage: wontfix`, move to **Won't Do**

---

## Special Cases

### Blocked Issues

If an issue is blocked by a dependency or decision:

1. Add `triage: blocked` label
2. Comment explaining what it's blocked by
3. Keep in current column (don't move to a priority column yet)
4. Link to blocking issue if applicable

### Security Issues

Security issues require special handling:

1. Add `security` label
2. Consider setting to `priority: must`
3. Be cautious in public comments (don't reveal exploit details)
4. Coordinate with maintainers on disclosure timing

### Questions

For `type: question` issues:

1. Answer the question in comments
2. If it reveals a bug or feature need, convert to appropriate type
3. If answered, close and move to **Done**
4. Consider if documentation needs improvement

### Discussions

For items needing team input:

1. Add `discussion` label
2. Keep in **Ready for Triage** column
3. Tag relevant team members
4. Once consensus is reached, proceed with normal triage

---

## Label Combinations Examples

Here are some common label combinations:

### Critical Bug
```
triage: confirmed
type: bug
priority: must
phase: 0 - pre-v1.0
```

### Important Feature for v1.x
```
triage: confirmed
type: feature
priority: should
phase: 1 - v1.x
```

### Nice Enhancement for Later
```
triage: confirmed
type: enhancement
priority: could
phase: future
```

### Good First Issue
```
triage: confirmed
type: bug
priority: should
phase: 0 - pre-v1.0
good first issue
```

### Needs Information
```
triage: needs info
type: bug
(no priority or phase yet)
```

### Rejected Feature
```
triage: wontfix
type: feature
priority: won't-do
```

---

## Triage Workflow Diagram

```
New Issue/PR
    ↓
[Inbox]
    ↓
Enough info? ──No──→ [Needs Information] + triage: needs info
    ↓ Yes
Valid? ──No──→ Close + triage: wontfix → [Won't Do]
    ↓ Yes
Add: triage: confirmed
Add: type: [bug|feature|enhancement|etc.]
    ↓
Need discussion? ──Yes──→ [Ready for Triage] + discussion
    ↓ No
Assign priority (MoSCoW)
Assign phase (0, 1, 2, future)
    ↓
Move to priority column:
- priority: must → [Must Do]
- priority: should → [Should Do]
- priority: could → [Could Do]
```

---

## Best Practices

### Be Prompt
- Triage new issues within 24-48 hours when possible
- Quick responses show active maintenance and encourage contributions

### Be Respectful
- Thank people for their contributions
- Explain decisions clearly and kindly
- Even when rejecting, be constructive

### Be Consistent
- Apply labels consistently across issues
- Follow this guide to maintain uniformity
- When in doubt, discuss with the team

### Be Thorough
- Read issues completely before triaging
- Ask for clarification rather than assuming
- Check for duplicates before creating new issues

### Keep It Organized
- Review the project board regularly
- Archive completed items periodically
- Re-evaluate priorities as the project evolves

### Document Decisions
- Comment on why you assigned specific labels
- Explain priority decisions for transparency
- Link related issues and discussions

---

## Regular Maintenance

### Weekly
- Review **Inbox** and **Needs Information** columns
- Follow up on stale items awaiting information
- Check **Must Do** isn't overloaded (max 5-10 items)

### Monthly
- Review **Could Do** items - do any need reprioritization?
- Check if **Phase** labels still align with roadmap
- Archive old items in **Done**

### Per Release
- Review all `phase: 0 - pre-v1.0` items before release
- Reassign incomplete items to appropriate future phases
- Update phases to reflect new roadmap versions

---

## Getting Help

If you're unsure about:
- **Priority assignment** - Discuss in issue comments or team channel
- **Phase alignment** - Reference [ROADMAP.md](ROADMAP.md) or ask maintainers
- **Technical validity** - Ask for input from relevant domain experts

---

## Related Documentation

- [ROADMAP.md](ROADMAP.md) - Project roadmap and phases
- [CONTRIBUTING.md](CONTRIBUTING.md) - Contribution guidelines
- [Issue #122](https://github.com/Jagalite/superseedr/issues/122) - Original label system proposal
- [Issue #153](https://github.com/Jagalite/superseedr/issues/153) - Roadmap phase labels proposal

---

**Last Updated:** January 2026
