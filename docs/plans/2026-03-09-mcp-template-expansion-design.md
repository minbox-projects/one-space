# MCP Templates Expansion Design (2026-03-09)

## Goal

Expand built-in MCP templates in OneSpace to provide a broader out-of-the-box catalog while keeping existing behavior stable.

Scope confirmed by user:

- Only add more built-in template entries.
- Use a mixed strategy (official ecosystem + practical third-party templates).
- Add 20 new templates.

## Constraints

- Keep existing template IDs and behavior unchanged.
- No changes to MCP form UI, Tauri command signatures, or storage schema.
- Use currently resolvable package names to avoid broken default commands.

## Approaches Considered

1. Official-first expansion (high stability, lower coverage)
2. Third-party-heavy expansion (high coverage, higher maintenance risk)
3. Balanced expansion (recommended and chosen)

Chosen approach: balanced expansion, combining official Model Context Protocol servers with practical third-party tools.

## Design

### Data Model

No struct changes. Reuse existing `MCPTemplate` fields:

- `id`, `name`, `description`
- `transport`, `command`, `args`, `url`
- `env_placeholders`, `headers_placeholders`
- `default_timeout`

### New Templates

Add these 20 templates:

1. `everything` -> `@modelcontextprotocol/server-everything`
2. `debug` -> `@modelcontextprotocol/server-debug`
3. `pdf` -> `@modelcontextprotocol/server-pdf`
4. `transcript` -> `@modelcontextprotocol/server-transcript`
5. `wiki-explorer` -> `@modelcontextprotocol/server-wiki-explorer`
6. `system-monitor` -> `@modelcontextprotocol/server-system-monitor`
7. `brave-search` -> `@modelcontextprotocol/server-brave-search`
8. `slack` -> `@modelcontextprotocol/server-slack`
9. `gitlab` -> `@modelcontextprotocol/server-gitlab`
10. `google-maps` -> `@modelcontextprotocol/server-google-maps`
11. `redis` -> `@modelcontextprotocol/server-redis`
12. `aws-kb-retrieval` -> `@modelcontextprotocol/server-aws-kb-retrieval`
13. `gdrive` -> `@modelcontextprotocol/server-gdrive`
14. `everart` -> `@modelcontextprotocol/server-everart`
15. `puppeteer` -> `@modelcontextprotocol/server-puppeteer`
16. `playwright` -> `@playwright/mcp`
17. `figma` -> `figma-mcp`
18. `linear` -> `linear-mcp-server`
19. `octocode` -> `octocode-mcp`
20. `weather` -> `@h1deya/mcp-server-weather`

### Defaults

- Transport: `stdio` for all new templates.
- Command format: `npx -y <package>`.
- Headers placeholders: empty for this batch.
- Timeout policy:
  - `60000` for general templates.
  - `120000` for heavier I/O/browser/integration templates.

### Environment Placeholder Strategy

Use practical placeholders for templates that usually require credentials:

- `brave-search`: `BRAVE_API_KEY`
- `slack`: `SLACK_BOT_TOKEN`
- `gitlab`: `GITLAB_PERSONAL_ACCESS_TOKEN`
- `google-maps`: `GOOGLE_MAPS_API_KEY`
- `redis`: `REDIS_URL`
- `aws-kb-retrieval`: `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_REGION`
- `gdrive`: `GDRIVE_CREDENTIALS_PATH`, `GDRIVE_OAUTH_PATH`
- `everart`: `EVERART_API_KEY`
- `figma`: `FIGMA_API_KEY`
- `linear`: `LINEAR_API_KEY`
- `octocode`: `GITHUB_TOKEN`

## Validation Plan

1. Run `cargo check` in `src-tauri`.
2. Confirm template list count increased from 7 to 27.
3. Confirm placeholder-bearing templates produce non-empty `env` in `get_mcp_template`.

## Risk and Mitigation

- Risk: some package CLIs evolve over time.
  - Mitigation: keep each template as a simple starting skeleton and allow manual adjustment in form UI.
- Risk: future package deprecations.
  - Mitigation: periodic maintenance of template list and docs.

## Implementation Note

The brainstorming skill requires handing off to writing-plans next. `writing-plans` is not available in this session's skill list, so implementation proceeds directly with this design doc as the approved plan artifact.
