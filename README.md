# Anki Decks

## Decks
[NVIDIA PTX ISA](https://docs.nvidia.com/cuda/parallel-thread-execution/),

## PTX
```sh
# Fetch the doc (one 3.5 MB page).
curl -o ptx.html https://docs.nvidia.com/cuda/parallel-thread-execution/index.html
# Build the deck.
nix run . -- ptx.html --apkg ptx-isa.apkg
# Or just the modules you're studying now.
nix run . -- ptx.html --apkg ptx-isa.apkg --modules core,concepts
```
`nix build .#ankit-mcp` also builds an MCP server for driving a running Anki
over AnkiConnect.

Then import the `.apkg` in Anki via File > Import.

Other output modes: `--json` dumps the parsed intermediate representation for
inspection, `--toml` emits the [ankit-builder](https://github.com/joshrotenberg/anki-toolkit)
deck definition that the `.apkg` is built from.

### Layout

```
PTX ISA
├── Core Instructions     integer/FP arithmetic, logic & shift, comparison,
│   ├── Recall            data movement, control flow, sync, stack, misc
│   ├── Syntax
│   ├── Qualifiers
│   └── Arch
├── Instructions          half/mixed precision, fabric, texture, surface,
│   └── ...               wmma, wgmma, tcgen05, video SIMD
└── Concepts              special registers, directives
    └── ...
```
