# Acknowledgements-rs

`acknowledge` is a CLI tool for generating list of acknowledgements based on your `Cargo.toml` file.

It works with github and gitlab APIs. **Only** public repos are supported.

## Install

```
cargo install acknowledgements-rs
```

## Usage

```
acknowledge is a simple CLI tool to analyze dependencies of a Cargo (rust) project and produce an ACKNOWLEDMENTS.md file listing (major) contributors of your dependencies

Usage: acknowledge [OPTIONS] --path <PATH> [COMMAND]

Commands:
  clear-cache  Clears cache
  help         Print this message or the help of the given subcommand(s)

Options:
  -p, --path <PATH>
          Path to Cargo project for analysis
  -g, --gh-token <GH_TOKEN>
          Running Acknowledgements on any project of reasonable size you're likely to face rate limits. Please provide a personal access token
  -o, --output <OUTPUT>
          Output file path, defaults to project path if not provided
  -f, --format <FORMAT>
          Format of the output file [default: NameAndCount]
  -d, --depth <DEPTH>
          Depth of scan, whether to include minor and optional depes contributors [default: Major]
  -c, --contributions-threshold <CONTRIBUTIONS_THRESHOLD>
          Min number of contributions to be included in the list, doesn't apply to sole contributors [default: 2]
  -s, --sources <SOURCES>
          List other sources, not specified in Cargo.toml
  -t, --template <TEMPLATE>
          Use your own template. See https://github.com/anvlkv/acknowledgements/blob/main/src/template.md?plain=1 for reference
  -h, --help
          Print help
  -V, --version
          Print version
```

### Options

#### Github access token

Be sure to provide one if you're waiting for limit reset for too long or too often.

#### Sources

Links any repos not discoverable via `Cargo.toml`

#### Depth

- `Major` - Non-optional dependencies
- `Direct` - All dependencies
- `Indepth` - All dependencies including `[build-dependencies]` and `[dev-dependencies]`

#### Format

- `NameAndCount` - Name of the contributor and count of contributions
- `DepAndNames` - Name of the dependency, names of contributors
- `NameAndDeps` - Name of the contributor, names of dependencies where they contributed


## Examples

- [`NameAndCount`](https://github.com/anvlkv/acknowledgements/blob/main/ACKNOWLEDGEMENTS.md) - `acknowledge -p .`
- [`DepAndNames`](https://github.com/anvlkv/acknowledgements/blob/main/ACKNOWLEDGEMENTS-DepAndNames.md) - `acknowledge -p . -f DepAndNames`
- [`NameAndDeps` ](https://github.com/anvlkv/acknowledgements/blob/main/ACKNOWLEDGEMENTS-NameAndDeps.md) - `acknowledge -p . -f NameAndDeps`
