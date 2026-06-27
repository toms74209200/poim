## Workflow

- **Before starting work**: Run `gh atat pull` to sync TODO.md with GitHub Issues
- **When creating a PR**: Always link the related issue
  - Use `gh pr create` with issue reference in body: `Closes #<issue_number>`
  - Example: `gh pr create --title "Implement feature" --body "Closes #1"`
- **After merging PR**: The linked issue will automatically close
- **When updating TODO.md**: Run `gh atat push` to sync changes to GitHub Issues

## Design and Implementation Guidelines

### Testing strategy

A module requires a responsibility, and responsibility means the module has verifiable behavior that can be specified as a test. This project favors false positives over false negatives; mocks are avoided because they risk false negatives. Pure logic is isolated into independent modules tested as small unit tests, and side effects are concentrated in `run.rs` and verified through acceptance tests.

### External spec vs. internal structure

Distinguish what has obvious consensus (e.g. devcontainer file paths defined by spec) from what requires discussion. External spec often provides obvious consensus, but not always — CLI options, error message wording, and similar decisions still require explicit judgment. Internal structure almost always requires discussion.

## Architecture Verification Workflow

When using server-architecture guidelines or writing new code:

### 1. Verify Guidelines Against Codebase
- Survey existing codebase to understand how each layer is implemented:
  - Review entire directory structure to identify which layer each belongs to
  - Extract implementation patterns (file naming, structure, dependencies) for each layer
  - Investigate reasons for any exceptional patterns

### Before Writing Tests
- Identify which architectural layer the code belongs to
- Confirm test strategy for that layer from guidelines
- **Review ALL existing test files in the same layer**:
  - File naming conventions
  - Build tag usage patterns
  - Directory placement
- Extract patterns by examining multiple files (minimum 3)
- Never rely on a single example

### Checklist Before Creating New Files
- [ ] Reviewed all existing files in the same directory
- [ ] Extracted naming convention patterns
- [ ] Understood reasons for any exceptional patterns
