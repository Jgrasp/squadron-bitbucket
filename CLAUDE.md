# squadron-bitbucket

The `bitbucket` extension of Squadron: Bitbucket Cloud pull requests and Pipelines runs, moved out of Squadron's core. Read `docs/PLAN.md` before anything. `../squadron-github` is the extension to copy.

This session works on this repo only. The plan's steps in `../squadron` (removing Bitbucket from the core) and in `squadron-extensions` (the `registry.json` entry, the Registry workflow) go to an agent on that project through `start_agent`, as the devkit rules say: never change, run or publish them from here. Same for any problem found in the app or the registry.

Load the `squadron-devkit:extension` skill before touching `extension.json` or the Rust code.

```bash
cargo build --release --target wasm32-wasip2
cargo test
```
