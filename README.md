# cortex

A terminal-first orchestration daemon for multi-agent AI workflows.

Cortex runs as a background daemon that watches your knowledge vault, routes tasks to the right AI supplier (Gemini, Claude Code, Codex), and surfaces approvals and results through an interactive TUI — without everything going through a single context window.

Cortex is based on obsidian brain that became relevant, but studing the implementation, I found a lot of gaps in terms of precision and token usage.

I decided to create cortex to use the best models for the correct aplication, and don't denpends of the IA to do everything in one shot, because even with well written agents,
a lot of tasks has been executed directly from the provider and model that received the task to distribute.

## Architecture

```
                    ┌─────────────────────────────┐
                    │         cortex daemon        │
                    │                              │
  vault writes ───► │  FileWatcher                 │
  hook events  ───► │  SocketReceiver  ──► Router  │
                    │                              │
                    └──────────────┬──────────────┘
                                   │ AppEvent stream
                    ┌──────────────▼──────────────┐
                    │          cortex tui          │
                    │  project tabs, approvals,    │
                    │  event log, review flow       │
                    └─────────────────────────────┘
```

**Lobes** are project domains (or some delegated functions). Each lobe has a path in the vault with structured subdirectories (`inquiries/`, `plans/`, `docs/`, `tasks/`).

**Suppliers** are the underlying execution providers: Gemini, Claude Code, Codex.

**Agents** are task-specific executors that map to a supplier.

The daemon and TUI are separate processes. The TUI connects to the daemon when you need to interact; the daemon keeps running regardless.

## Inquiry flow

Drop a markdown file with YAML frontmatter into `<lobe>/inquiries/`. The daemon detects it, routes it to the right supplier, writes the result to `<lobe>/docs/`, and updates the inquiry status.

```yaml
---
type: inquiry
id: INQ-001
title: What are the tradeoffs between X and Y?
kind: research        # research | decision | analysis
status: pending       # daemon picks this up
output: docs/x-vs-y
---

Context and question body here.
```

| Kind | Supplier |
|------|----------|
| `research` | Gemini |
| `decision` | Opus |
| `analysis` | Opus or Gemini |

## Installation

```bash
cargo build --release
cp target/release/cortex ~/.local/bin/
```

Copy and edit the example config:

```bash
cp cortex.example.toml ~/.config/cortex/cortex.toml
```

Optionally install as a systemd user service:

```bash
cp cortex.service ~/.config/systemd/user/
systemctl --user enable --now cortex
```

## Configuration

`cortex.toml` is gitignored. See `cortex.example.toml` for the full schema.

```toml
[monitor]
socket_path = "/tmp/cortex.sock"

[[lobes]]
name = "my-project"
path = "~/vault/projects/my-project"

[[suppliers]]
name = "gemini"
type = "gemini"

[[suppliers]]
name = "claude-code"
type = "claude-code"
```

## Usage

```bash
cortex daemon          # start the background daemon
cortex tui             # open the interactive TUI
cortex event '<json>'  # send an event (used by editor hooks)
```

### TUI keybindings

| Key | Action |
|-----|--------|
| `Tab` / `→` | Next lobe |
| `Shift+Tab` / `←` | Previous lobe |
| `n` | New inquiry |
| `a` | Accept pending review |
| `r` | Reject pending review |
| `q` | Quit |

## Hook integration

Wire your editor hooks to send events to the daemon:

```bash
echo '{"type":"agent_started","desc":"elixir-dev"}' | nc -U /tmp/cortex.sock
```

## Requirements

- Rust 1.77+
- `gemini` CLI (for research inquiries)
- `claude` CLI (for decision/analysis inquiries)
- `obsidian` CLI (for vault writes)
