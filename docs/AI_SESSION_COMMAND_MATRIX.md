# AI Session Command Matrix (Baseline)

Checked date: `2026-03-13`

## Launch commands

### Claude
- Create: `claude --session-id <session_id>`
- Resume: `claude -r <session_id>`
- Create-time custom `session_id`: supported

### Gemini
- Create: `gemini`
- Resume: `gemini -r <uuid|number|latest>`
- Create-time custom `session_id`: not supported

### Codex
- Create: `codex`
- Resume: `codex resume <session_id>`
- Create-time custom `session_id`: not supported

### OpenCode
- Create: `opencode`
- Resume: `opencode -s <session_id>`
- Create-time custom `session_id`: not supported

## Baseline versions

- Claude CLI package: `@anthropic-ai/claude-code` -> `2.1.74`
- Gemini CLI package: `@google/gemini-cli` -> `0.33.1`
- Codex CLI package: `@openai/codex` -> `0.114.0`
- OpenCode CLI package: `opencode-ai` -> `1.2.25`

