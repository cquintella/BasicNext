# Basic Next 0.2 Release Restoration Bucket

This is the active delivery bucket for restoring the **0.2** release after the
GitHub repository was recreated. The implementation plan is
[`WBS-0.2.md`](WBS-0.2.md).

Authority, in order: [`0.2.ebnf`](../docs/language/0.2/0.2.ebnf),
[`0.2.md`](../docs/language/0.2/0.2.md), and
[`keywords.md`](../docs/language/0.2/keywords.md).

## Release restoration

- [x] Diagnose the missing GitHub Release and failing tag workflow.
- [x] Restore repository-internal links required by the quality gate.
- [x] Make tagged release publication safe to rerun.
- [x] Pass the complete local quality gate.
- [ ] Publish the repaired `v0.2.0` tag and binary assets.
- [ ] Verify release checksums and GitHub Actions results.

## Preserved project records

- [`bucket-0.2.md`](../archive/project/bucket-0.2.md) — archived 0.2 remediation program.
- [`bucket-0.3-planning-draft.md`](../archive/project/bucket-0.3-planning-draft.md) — incomplete 0.3 planning draft recovered from the recreated repository.
- [`gap_analysis.md`](gap_analysis.md) — reconciled implementation gaps.

The 0.3 plan remains archived until its WBS and normative language documents
are restored and reviewed together.
