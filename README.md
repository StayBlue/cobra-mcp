# cobra-mcp

An [MCP](https://modelcontextprotocol.io) server for [CoBRA](https://github.com/trailofbits/CoBRA),
Trail of Bits' Mixed Boolean-Arithmetic (MBA) expression simplifier. It lets an
MCP client deobfuscate expressions such as `(x&y)+(x|y)` → `x + y`.

CoBRA is compiled into the `cobra-mcp` binary, so nothing else needs to be
installed.

## Setup

Download a release archive (or [build it](#building)), put `cobra-mcp` on your
`PATH` and add it to your client's configuration, e.g. `.mcp.json`:

```json
{ "mcpServers": { "cobra": { "command": "cobra-mcp" } } }
```

## Tools

### `simplify`

| Parameter    | Default | Description                                                                        |
|--------------|---------|------------------------------------------------------------------------------------|
| `expr`       |         | Expression using `+ - * ** & \| ^ ~`, `<<`/`>>` by constants, constants, variables |
| `bitwidth`   | 64      | Width of the modular arithmetic, 1–64                                              |
| `max_vars`   | 16      | Maximum number of distinct variables, 1–20                                         |
| `timeout_ms` | 30000   | Time limit, up to 600000                                                           |

The result is structured: `status` (`simplified` or `unsupported`), the
resulting `expr`, the sorted `input_variables`, CoBRA's `semantic_class` for the
input, `spot_checked` (whether the result passed CoBRA's random-input check)
and, when CoBRA could not simplify, a `reason`. Parse errors, invalid parameters
and timeouts are returned as tool errors.

Each call runs in a child process of `cobra-mcp` (`cobra-mcp --worker`), so a
call that exceeds its time limit is killed without affecting the server.

## Building

Requirements: Rust 1.88+, CMake 3.20+, a C++23 compiler (Clang 17+, GCC 14+ or
recent MSVC) and network access on the first build, when CoBRA's dependency
build fetches Abseil and Highway.

```sh
git clone --recursive <this repository>
cd cobra-mcp
cargo build --release
```

CoBRA is pinned as a submodule at `vendor/CoBRA`; if you cloned without
`--recursive`, run `git submodule update --init` first.

Z3-backed verification (`cobra-cli --verify`) is not built in yet.

## License

cobra-mcp is licensed under the [MIT License](LICENSE). Third-party license
notices are included in release archives under `licenses/`.
