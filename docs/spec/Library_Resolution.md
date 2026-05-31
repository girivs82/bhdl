# Library Resolution & Project Manifests — spec v0

> **Status:** Proposal v0. Defines how BHDL resolves `import { … } from
> "<namespace>/path.bhdl"` statements against declared libraries —
> built-in, third-party, and **proprietary/internal** — in a
> reproducible, Cargo-style way.
>
> **Motivation:** companies generate proprietary stdlibs and keep them
> internal; they must be addable without vendoring into the BHDL repo
> and without brittle absolute paths, while keeping builds reproducible
> across machines and CI (you must be able to rebuild the exact netlist
> you fabbed).

## 1. The problem with what exists today

Two disjoint resolvers, neither reproducible for third-party libs:

- **Import resolver** (`bhdl-synthesizer/src/import_loader.rs`) — resolves
  `./` / `../` relative to the importing file; everything else is a
  literal path from cwd. `import … from "bhdl-stdlib/…"` only works
  because tests run at the workspace root. No search path, no env var.
- **Component-library resolver** (`bhdl-analyzer/.../component_library/
  resolver.rs`) — honours `BHDL_STDLIB_PATH` + exe-relative + cwd, but
  only for the component *catalog*, not import statements.

Consequence: a proprietary library can only be referenced by hardcoded
absolute path in every `import`. Not portable, not reproducible, and a
same-named part on a different machine's path could silently resolve to
a different SKU — unacceptable for a tool that emits fabbable hardware.

## 2. Design principles (Cargo-shaped)

1. **Declared, not ambient.** A board's library dependencies are
   declared in a version-controlled **project manifest** (`bhdl.toml`),
   exactly like `Cargo.toml`'s `[dependencies]`. Imports resolve *only*
   against declared libraries — not a free-for-all of whatever happens
   to be on a search path. This is what makes the dependency set
   explicit and the build reproducible.
2. **Declaration ≠ location.** The manifest says *which* libraries (by
   name + version). *Where* their roots live on this machine comes from
   an explicit `path =`, or — for name-only deps — a search path
   supplied by CLI (`-I` / `--lib-path`, authoritative) or env
   (`BHDL_LIB_PATH`, ambient fallback). Same split as Cargo's
   manifest-vs-registry/`paths`.
3. **Namespaced imports.** Every import is `<libname>/<path-in-lib>.bhdl`.
   The `<libname>` prefix is resolved against the declared libraries.
   `bhdl-stdlib` is the reserved namespace for the bundled lib (implicit,
   always available — BHDL's "std").
4. **Library identity is self-declared.** Each library root carries a
   `manifest.toml` (`[library] name, version`) — the existing per-lib
   manifest format. Resolution version-checks the declared dependency
   against the library's own manifest; mismatch is a hard error.
5. **Reproducibility is non-negotiable.** CLI flag > env var > bundled,
   and imports only resolve to declared+version-matched libs. A lockfile
   (`bhdl.lock`) pinning resolved roots is a v1 addition; the v0
   manifest + version check already prevents silent SKU drift.

## 3. The two manifests

### 3.1 Project manifest — `bhdl.toml` (new)

Lives next to the board source, version-controlled. Discovered by
walking up from the input file's directory (Cargo-style), or named
explicitly with `--manifest <path>`.

```toml
[project]
name    = "acme-sensor-board"
version = "0.3.0"

[libraries]
# Built-in `bhdl-stdlib` is implicit and need not be listed.
#
# By explicit path (proprietary lib checked out at a known location):
acme-stdlib = { path = "../acme-bhdl-libs/acme-stdlib", version = "2.1" }
#
# By name + version, resolved against the search path (-I / $BHDL_LIB_PATH):
sensor-lib  = { version = "1.4" }
#
# Shorthand: bare version string == { version = "…" }, name-resolved:
fpga-lib    = "0.9"
```

- `path` (optional) — explicit root dir, relative to the manifest.
- `version` (required) — exact match in v0 (`"2.1"` means `==2.1.x`
  patch-flexible; full semver ranges are v1). Checked against the
  library root's own `manifest.toml`.
- A dep with no `path` is resolved by name against the search path
  (§4): the first root whose `manifest.toml` declares
  `name = "<dep>"` *and* a matching version wins.

### 3.2 Library manifest — `manifest.toml` (existing, reused)

Already shipped for `bhdl-stdlib`:

```toml
[library]
name    = "acme-stdlib"
version = "2.1.0"
```

Its presence marks a directory as a library root. `name` is the
namespace imports use (`import … from "acme-stdlib/…"`); `version` is
what the project manifest's dependency check compares against.

## 4. Resolution algorithm

For an import `from "<ns>/<rel>.bhdl"`:

1. **`<ns>` == `bhdl-stdlib`** → the bundled library (located as today:
   exe-relative, then cwd, then `BHDL_STDLIB_PATH`). Always available
   without a manifest entry.
2. **Otherwise** `<ns>` must be a declared dependency in `bhdl.toml`
   `[libraries]`. If not declared → error: *"import references library
   `<ns>` which is not declared in bhdl.toml [libraries]"*.
3. Resolve the declared dep to a root dir:
   - `path =` given → that dir (relative to the manifest).
   - else → search each root in order: explicit `-I`/`--lib-path` dirs
     (in CLI order), then `$BHDL_LIB_PATH` (colon-separated). A root
     *matches* if `root/manifest.toml` has `name == <ns>`.
   - First match wins; report all searched roots on miss.
4. **Version-check** the root's `manifest.toml` version against the
   declared `version`; mismatch → hard error naming both.
5. Read `<root>/<rel>.bhdl`.

`./` and `../` imports keep their current file-relative behaviour
(co-located fragments, no namespace, no manifest needed).

**Precedence** for name-resolved roots: `-I` flags > `$BHDL_LIB_PATH`.
CLI is authoritative (lives in the build script → reproducible); env is
the ambient "installed once per dev image" convenience.

## 5. CLI surface

```
bhdl synth board.bhdl \
    --manifest path/to/bhdl.toml \   # default: discover by walking up
    -I /opt/acme/bhdl-libs \         # repeatable lib search root
    -I ~/work/sensor-libs
# env fallback:
BHDL_LIB_PATH=/opt/acme/bhdl-libs:/usr/share/bhdl-libs  bhdl synth board.bhdl
```

- No `bhdl.toml` found and only `bhdl-stdlib` imports used → works
  (manifest optional for stdlib-only boards; back-compat).
- No `bhdl.toml` but a non-stdlib import → error directing the user to
  create one. Explicit beats silent.

## 6. Why not just a search path (no manifest)?

A bare search path (the `-I`-only / env-only design) resolves
`acme/x.bhdl` to *whatever's first on the path* — which differs per
machine and silently changes which SKU you fab. The manifest makes the
dependency set **declared and version-pinned**, so resolution is
reproducible and reviewable in version control. The search path then
only supplies *where the declared libs live* on a given machine — it
cannot introduce an undeclared dependency.

## 7. Implementation stages

1. **Manifest types + parsing** (`bhdl-common`): `ProjectManifest`
   (`[project]`, `[libraries]`), reuse the existing `LibraryManifest`
   (`[library]`). Pure types + serde + a `discover()` that walks up for
   `bhdl.toml`.
2. **`LibraryResolver`** (`bhdl-common` or `bhdl-synthesizer`): builds
   the namespace→root map from (manifest, `-I` roots, env), with the
   version check. One resolver both the import loader and the analyzer
   component resolver consume — unifies today's two mechanisms.
3. **Import loader** wired to the resolver: `import_loader.rs` asks the
   resolver to turn `<ns>/<rel>` into a file path.
4. **CLI**: `--manifest`, `-I`/`--lib-path`; thread resolver config into
   `NetlistGenerator`.
5. **Analyzer component resolver** folded onto the same root list (drop
   the parallel `BHDL_STDLIB_PATH`-only logic; keep the var as an alias
   for locating the bundled lib).
6. **End-to-end test**: a fixture proprietary lib (`acme-stdlib/` with
   its own `manifest.toml` + one entity) declared in a `bhdl.toml`,
   imported and synthesized, resolved via (a) explicit `path =`,
   (b) `-I` flag, (c) `$BHDL_LIB_PATH` — plus the negative cases
   (undeclared namespace, version mismatch, missing root).

## 7a. Lockfile — `bhdl.lock` (landed)

The version field in `bhdl.toml` is necessary but not sufficient for
multi-year reproducibility: v0 version matching is patch-flexible (a
`"2.1"` pin accepts any `2.1.x`), and — worse — a vendor can edit a
recipe *in place* without bumping the version at all (the literal
"10 kΩ pulldown silently becomes 15 kΩ" case). The lockfile closes
both gaps.

`bhdl.lock` sits next to `bhdl.toml`, is committed, and pins for every
declared library:

```toml
version = 1

[[library]]
name    = "acme-stdlib"
version = "2.1.0"                 # exact resolved version, not the loose pin
hash    = "md5:9f3a…"             # content digest of the library root
source  = "path:../acme/acme-stdlib"
```

- **Content hash** = md5 over every `.bhdl` file + `manifest.toml` in the
  library root, visited in sorted relative-path order, each framed by
  its path + length. Deterministic across machines/time. md5 is for
  **drift detection, not security** — we're catching an accidental
  vendor edit, not a crafted collision. The hash is the part that
  catches an in-place change with no version bump.
- **Lock pins the whole declared set** (like Cargo.lock), not just what
  one board imports — a pure function of (manifest + search path).

### Enforcement

| State | Default | `--locked` (CI) | `--update-lock` |
|---|---|---|---|
| no lock | generate + write | **error** (commit a lock first) | generate |
| lock matches | build | build | regenerate |
| lock drifted | **error** (loud) | **error** | regenerate |

Drift is classified: `Content` (same version, different bytes — the
dangerous silent case), `Version`, `Added`, `Removed`. Default mode
refuses to build on any drift and names the offending library; the user
either restores the locked library or passes `--update-lock` to accept
the change intentionally. **Never a silent substitution** — the
property that matters for fabbable hardware.

A 4-year rebuild therefore either reproduces the byte-identical library
or fails loudly ("library `acme-stdlib` (2.1.0) CONTENT changed since
the lock"), pointing the user at the exact divergence.

> **Still requires the library to be retrievable.** The lock guarantees
> *detection* of drift and *exact-version selection*; it does not by
> itself archive the library bytes. Pair it with the company archiving
> its tagged library versions (or a future registry/vendor-dir), and
> with the complementary frozen-structural-netlist release artifact for
> the absolute as-fabbed record. (See the reproducibility discussion in
> the project notes.)

## 7b. Frozen structural netlist — `bhdl freeze` (landed)

The lockfile makes a build *reproducible from source*. Its complement
is the **frozen structural netlist**: the *result* of a build —
every concrete component (resolved value/footprint/MPN) and the flat
connectivity, after all expansion/design/parametric inference — but
**without** the recipes, intents, or templates that produced it. It's
the manufacturing/release record ("this exact netlist is what we
fabbed"), self-describing and dependency-pinned, and it depends on
nothing being retrievable later.

```
bhdl board.bhdl freeze -o board.frozen.json
```

The schema is **stable and versioned** (`schema_version`), unlike
`synth --format json` (which dumps the internal netlist verbatim):

- **`provenance`** — toolchain version, source path, RFC-3339
  timestamp, and the embedded **lockfile libraries** (name + exact
  version + content hash) the build resolved against, so the frozen
  record alone documents its full dependency set.
- **`components`** — sorted by refdes; `refdes`, `component_type`,
  resolved `value`/`footprint`/`mpn`, plus curated structural
  attributes. Synthesis-internal and design-layer keys (`intf_*`,
  `vpin_*`, `expansion_*`, `alias__*`, intents) are stripped — this is
  the as-fabbed structural record, not the design model.
- **`nets`** — sorted by name; each carries its `(refdes, pin)`
  endpoints (sorted). Flat, no hierarchy.

It is a **derived snapshot, not rebuildable source** (no design layer
to re-run). The two artifacts are complementary: keep the lockfile for
reproducible rebuilds and the frozen netlist for the immutable
as-fabbed record. Implementation: `bhdl-synthesizer/src/freeze.rs`.

## 8. Out of scope for v0

- **Registry / vendor-dir** that archives library bytes (the lock
  detects drift but doesn't store the bytes) — v1+.
- **Registry / network fetch** — v1+; v0 is path/search-root only.
- **Git dependencies** — later.
- **Full semver ranges** (`^`, `~`, `>=`) — v0 is exact-with-patch-flex;
  ranges are v1.
- **Transitive library deps** (a lib depending on another lib) — v0
  assumes flat; revisit if a real proprietary lib needs it.
