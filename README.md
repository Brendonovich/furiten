# Furiten

Flashcards you write in Markdown and review with spaced repetition. Works offline, syncs across devices, and agents
can use it through a CLI, an HTTP API and an MCP server.

```
What is the capital of France?
---
Paris
```

```sh
cargo run -p furiten -- add --deck Geography "What is the capital of France?
---
Paris"
cargo run -p furiten -- review
cargo run -p furiten -- cram --deck Geography   # practice every card, due or not; nothing is scheduled
cargo run -p furiten-desktop
```

See [PLAN.md](PLAN.md) for how it fits together and [docs/contract.md](docs/contract.md) for the API.
