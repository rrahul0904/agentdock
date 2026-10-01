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
