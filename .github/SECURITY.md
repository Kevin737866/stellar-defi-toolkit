# Security Policy

Stellar DeFi Toolkit contracts hold or route user funds. If you find a
vulnerability, please report it privately rather than opening a public issue or
pull request — public disclosure before a fix ships can put user funds at risk.

## Reporting a Vulnerability

**Do not open a public GitHub issue for a security vulnerability.**

Instead, use GitHub's private vulnerability reporting for this repository:
[Security Advisories → Report a vulnerability](../../security/advisories/new).
If that's unavailable to you, contact a maintainer directly through a private
channel (e.g. a direct message) rather than a public issue, PR comment, or
Discord channel.

Please include:

- A description of the vulnerability and its potential impact
- The affected contract(s) and function(s) — cross-reference
  [`docs/ACCESS_CONTROL_MATRIX.md`](../docs/ACCESS_CONTROL_MATRIX.md) if it maps to a
  known access-control gap
- Steps to reproduce, or a minimal proof-of-concept (test case, script, or
  transaction trace)
- Any suggested remediation, if you have one

## What to Expect

- **Acknowledgment:** we aim to acknowledge new reports within 3 business days.
- **Triage:** we'll assess severity using the categories in
  [`docs/SECURITY_AUDIT_CHECKLIST.md`](../docs/SECURITY_AUDIT_CHECKLIST.md)
  (reentrancy, oracle manipulation, flash loan attacks, governance attacks, access
  control, and standard DoS/overflow classes) and confirm whether it's a new finding
  or already tracked in that document's known-findings appendix.
- **Fix and disclosure:** once a fix is merged (or a mitigation is in place), we'll
  coordinate on public disclosure timing with the reporter. We do not have a bug
  bounty program at this time, but we will credit reporters (with permission) in
  release notes.

## Scope

In scope:

- All contracts under `src/contracts/`, `governance/`, and `staking/`
- Access control and authentication logic
- Oracle price aggregation and consumption
- Arithmetic in fee, interest, collateral-ratio, and reward calculations
- Governance proposal/voting/execution logic

Out of scope:

- Issues already listed in
  [`docs/SECURITY_AUDIT_CHECKLIST.md` Appendix A](../docs/SECURITY_AUDIT_CHECKLIST.md#appendix-a-current-known-findings-by-severity)
  — these are known and tracked; feel free to reference them, but they don't need a
  fresh private report.
- The CLI (`src/main.rs`, `cli/`) and example code (`examples/`), unless the issue
  demonstrates a path back into contract state or fund custody.
- Denial of service against third-party infrastructure (RPC providers, Horizon, etc.)
  rather than this codebase.

## Supported Versions

This project is pre-1.0 and does not yet maintain multiple supported release
branches. Security fixes land on `main`; there is no backport policy until a 1.0
release establishes one.

## Review Cadence

Beyond ad hoc reports, the protocol's overall attack surface is reviewed quarterly
per the cadence defined in
[`docs/SECURITY_AUDIT_CHECKLIST.md`](../docs/SECURITY_AUDIT_CHECKLIST.md#4-review-cadence).
