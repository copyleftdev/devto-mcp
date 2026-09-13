# Releasing

A release is one tag. Everything else follows from it.

```sh
git tag -a v0.1.0 -m "devto-mcp 0.1.0"
git push origin v0.1.0
```

That runs `.github/workflows/release.yml`, which:

1. builds for five targets — Linux x86_64 and aarch64, macOS arm64 and x86_64, Windows x86_64
2. **smoke-tests each binary on its own platform**: start it, complete the handshake, list the
   tools. A binary that compiles and cannot be registered is not a release.
3. assembles one `.mcpb` bundle carrying every target, and unpacks and runs it to check the
   launcher resolves the right build
4. stamps the bundle's URL and SHA-256 into a copy of `server.json`
5. publishes the archives, the bundle, `server.json` and `SHA256SUMS` to a GitHub release

## Publishing to the MCP registry

The registry entry is not automated, because it needs an interactive GitHub login and the
namespace is proof of ownership rather than a setting.

```sh
# The stamped copy from the release, not the one in the repository — that one carries a
# deliberate placeholder hash so an unstamped publish fails loudly.
gh release download v0.1.0 --pattern server.json --output server.json

mcp-publisher login github          # proves io.github.copyleftdev is yours
mcp-publisher publish               # reads server.json from the working directory
```

`io.github.copyleftdev/*` is the namespace GitHub authentication grants. Publishing under any
other namespace needs DNS or HTTP proof of the domain instead.

## What to check before tagging

- `./scripts/gate.sh` is green — fmt, clippy, the suite, and mutation testing to zero
  survivors
- `CHANGELOG.md` has an entry for the version
- the version in `Cargo.toml`, `server.json` and `mcpb/manifest.json` agree; the workflow
  stamps the latter two from the tag, so the tag is what actually decides
- CI's `manifests` job is green, which is what keeps `server.json` inside the published
  schema and the bundle's declared tools equal to the tools the server really serves
