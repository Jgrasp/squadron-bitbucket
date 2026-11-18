# squadron-bitbucket

The `bitbucket` extension of Squadron: Bitbucket Cloud pull requests and Pipelines runs, moved out of Squadron's core. Read `docs/PLAN.md` before anything: the work spans this repo, `../squadron` and `../squadron-extensions`. `../squadron-github` is the extension to copy.

Load the `squadron-devkit:extension` skill before touching `extension.json` or the Rust code.

```bash
cargo build --release --target wasm32-wasip2
cargo test
```
