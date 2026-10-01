# Supervisor Console

AgentDock Supervisor Console is the local Codex-style operator shell for the Self Working Agent / Local Astra stack.

## Architecture split

- AgentDock owns the interactive local operator surface, project/session/service visibility, and future approved operator actions.
- Autonomous Forge owns portfolio reasoning, scheduling, coding workers, verification, dogfood metrics, and supervisor decisions.

Phase A is deliberately read-only. It consumes a versioned `supervisor-snapshot/v1` JSON file and does not claim live Autonomous Forge connectivity yet.

## One-shot dashboard

```bash
cargo run -p agentdock-cli -- supervisor --snapshot docs/examples/supervisor.snapshot.example.json
```

JSON output:

```bash
cargo run -p agentdock-cli -- supervisor --snapshot docs/examples/supervisor.snapshot.example.json --json
```

## Interactive console

```bash
cargo run -p agentdock-cli -- console --snapshot docs/examples/supervisor.snapshot.example.json
```

## Live watch

When Autonomous Forge is publishing the same snapshot path after daemon lifecycle changes and cycles:

```bash
cargo run -p agentdock-cli -- watch --snapshot /path/to/autonomous-forge/.ai/supervisor/snapshot.json
```

The default poll interval is 1000ms. Override it within the bounded 100–60000ms range:

```bash
cargo run -p agentdock-cli -- watch --snapshot /path/to/snapshot.json --interval-ms 2000
```

Watch mode reloads the strict snapshot and prints only when validated supervisor state changes. Invalid or partially replaced files are refused and retried on the next poll.

Commands:

- `status` — supervisor, queue and machine-pressure summary
- `projects` — project status, blockers and next actions
- `workers` — configured/active workers and assignments
- `tasks` — durable work queue view
- `risks` — current risk/attention items
- `decisions` — recent supervisor decisions
- `refresh` — reload the same snapshot file
- `help` — command help
- `quit` / `exit` — leave the console

## Safety boundary

Phase A does not:

- execute shell commands;
- start/stop processes;
- mutate tasks or priorities;
- send messages to other agents/chats;
- merge/push/deploy code;
- read Autonomous Forge SQLite internals directly.

The next verified slice should add a narrow read-only snapshot export adapter from Autonomous Forge. Only after that evidence should AgentDock gain receipt-backed mutation commands such as pause/resume, reprioritize or approve a previously observed remediation target.

## Receipt-backed controls

Phase B can author four typed local requests for Autonomous Forge:

```bash
agentdock control pause-project applyai --root /path/to/autonomous-forge --confirm
agentdock control resume-project applyai --root /path/to/autonomous-forge --confirm
agentdock control set-project-priority applyai 1 --root /path/to/autonomous-forge --confirm
agentdock control set-task-priority RE-347 0 --root /path/to/autonomous-forge --confirm
```

`--request-id <id>` is optional when an external automation needs a stable idempotency key. Otherwise AgentDock generates one.

AgentDock only writes a strict `supervisor-control/v1` JSON request under `.ai/supervisor/requests/`. It does not open Forge SQLite and does not execute `forge`, a shell, Git, Docker, or any target process.

Forge validates and applies the request on a daemon cycle, writes a durable receipt, archives the request, and republishes the supervisor snapshot. `pause-project` stops new scheduling only; it does not terminate work already in flight.
