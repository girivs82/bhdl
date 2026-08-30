# Source Resolvers (level-3 auto-fetch) — scoping spec v0

> **Status:** Core landed (the helper protocol + cache + fetch +
> lock/hash verification — §3/§4, schema §2). Remaining: a built-in
> `git` convenience scheme and the `--offline`/`bhdl fetch`/allowlist
> hardening (§6/§5). Defines the optional level-3 of library resolution
> (`Library_Resolution.md` §7c): letting
> the toolchain *fetch* a declared library from a VCS/remote by a
> pinned revision, instead of the user syncing it and pointing `path =`
> at it (levels 1–2, which already work).
>
> **Driving constraint:** a company on Perforce — or a git+p4 shop —
> must be able to add their own fetch mechanism **without modifying or
> recompiling BHDL**. So the resolver layer is an *external, language-
> agnostic, VCS-agnostic* extension point. BHDL never speaks any VCS
> protocol itself.

## 1. Goals / non-goals

**Goals**
- Toolchain-driven fetch of a declared library at a pinned revision.
- VCS-agnostic + extensible: git, Perforce, SVN, an internal artifact
  store — all addable as drop-in helpers, none privileged in core.
- Reproducible + verifiable: fetched bytes are checked against the
  lockfile content-hash; a tampered/moved revision fails loudly.
- Dedup + cache: a given (source, rev) is fetched once, content-
  addressed, reused across boards.
- Hermetic CI: `--offline` + `--locked` build entirely from cache, no
  network.

**Non-goals**
- BHDL implementing any VCS protocol. It shells out; helpers do the work.
- A package registry (explicitly rejected — VCS is the archive).
- Dependency *version solving* across a graph (still flat per §8); the
  rev is pinned, not solved.
- Auth/credential management — delegated to the helper + the user's
  existing VCS credentials.

## 2. Source-spec schema

A dependency gains an optional `source` + `rev`. `version` stays
(checked against the fetched library's `manifest.toml`).

```toml
[libraries]
# git
acme-stdlib = { source = "git:ssh://git.acme/bhdl-libs/acme-stdlib", rev = "v2.1.0", version = "2.1" }
# Perforce (helper supplied by the company)
sensor-lib  = { source = "p4://depot/bhdl-libs/sensor-lib/...",       rev = "1287634", version = "1.4" }
# internal artifact store
fpga-lib    = { source = "acmestore:fpga-lib",                        rev = "sha256:9c…", version = "0.9" }
```

- `source` = `<scheme>:<locator>`. The **scheme** selects the resolver
  (§3); the **locator** is opaque to BHDL, meaningful only to the helper.
- `rev` = the pinned revision. **MUST be immutable** — a commit SHA, a
  Perforce changelist number, a content digest. Mutable-looking refs
  (`main`, `HEAD`, `latest`, …) are flagged with a warning and the
  fetch proceeds (the content-hash catches any resulting drift); only
  an empty `rev` is a hard error.
- A dependency with `source` is *fetched*; one with `path =` is local;
  one with neither is name-resolved on the search path (level-1/2,
  unchanged). `source` and `path` are mutually exclusive.

The lockfile already carries a free-form scheme-prefixed `source`
string and an exact `version` + content `hash`; it gains a `rev` field.
No breaking change to the lock format beyond the added field.

## 3. The resolver helper protocol (the heart)

A resolver is an **external executable**, discovered by name —
`bhdl-source-<scheme>` on `PATH` (or in a configured resolver dir). This
mirrors git's `git-<cmd>` / cargo's credential helpers / Docker's
CSI-style plugins: language-agnostic, no ABI, no recompile, no plugin
crate. A company's p4 resolver is a ~20-line shell script.

**Invocation.** BHDL spawns the helper with a JSON request on **stdin**
and expects a JSON response on **stdout** (room to grow without arg
churn):

```jsonc
// stdin → bhdl-source-p4
{
  "protocol": 1,
  "locator": "//depot/bhdl-libs/sensor-lib/...",
  "rev":     "1287634",
  "dest":    "/home/u/.cache/bhdl/sources/tmp-abc123",  // BHDL-owned scratch dir
  "offline": false
}
```
```jsonc
// stdout ← bhdl-source-p4  (on success, exit 0)
{ "ok": true }
```

A response body is optional. When one is present, BHDL reads only
`ok` (+ `message` on failure); any other field — including an echoed
`protocol` — is ignored (`source.rs`, `HelperResponse`). A nonzero
exit fails the fetch regardless of the body.

Contract:
- The helper populates `dest` with the **library root** (the directory
  containing `manifest.toml`) at exactly `rev`, then exits 0.
- Nonzero exit (or `ok:false` + a `"message"`) = fetch failed; BHDL
  surfaces the helper's stderr/message verbatim.
- **Determinism:** same `(locator, rev)` ⇒ byte-identical `dest`. The
  helper must not depend on ambient state. BHDL enforces this *post hoc*
  via the content-hash, but the contract states it.
- The helper does all networking/auth. BHDL does none.
- `offline:true` ⇒ the helper must not hit the network (fail if not
  locally satisfiable); BHDL also won't call a helper at all when the
  cache already satisfies the rev (§4), so `offline` is a belt-and-
  suspenders signal.

**Built-in `git`.** Because git is near-universal, BHDL *may* ship a
built-in `git` scheme (shelling `git` directly) so the common case needs
no helper install. Everything else — `p4`, `svn`, `acmestore`, … — is a
helper. (Decision §8.) Even `git` could be expressed as the reference
helper for uniformity; leaning built-in for ergonomics.

## 4. Cache + resolution flow

Cache root (`source::cache_root`): `$BHDL_CACHE/sources` if set, else
`~/.cache/bhdl/sources`, else a process-local temp dir. A pinned
source's cache dir (`spec_cache_dir`) is
`<cache_root>/<scheme>/<key>` where `<key>` is the first 16 hex chars
of `sha256(locator + "@" + rev)` — computable from `bhdl.toml` alone,
*before* any fetch (it keys the request, not the fetched content), and
laid out per-scheme for human browsability.

Resolving a `source` dependency (`source::resolve_source`):
1. If the cache dir's `manifest.toml` exists → use it. **No fetch, no
   helper, works offline.** The cache hit checks only that presence —
   it does NOT re-hash the tree; content verification against the
   locked sha256 is the caller's separate job (`hash_library_root` vs
   the lock, the ordinary `Content`-drift gate).
2. Else, if offline was requested → hard error (not cached, fetch
   forbidden).
3. Else fetch: spawn `bhdl-source-<scheme>` into a scratch dir under
   `<cache_root>/.tmp/`; require it produced a `manifest.toml`; then
   atomically `rename` the scratch dir into the cache slot (a
   concurrent build that won the race is tolerated).
4. The caller then verifies as for any root: content hash against the
   lock (mismatch = `Content` drift — a remote served different bytes
   for the same rev, or a mutable rev moved; first build records the
   hash), and the library's `manifest.toml` version against the
   declared `version`.
5. The import loader proceeds on the cache path as for any root.

This composes cleanly with levels 1–2: `path =`/search resolution is
unchanged; only `source` deps go through the fetch+cache path.

## 5. Security / trust model

- **Helpers run with user privileges** — they're executables the user
  installed, same trust as any build tool. Documented boundary, not a
  sandbox.
- **Tamper-evidence vs a hostile remote** is the content-hash. The
  shipped lock hash is **sha256** (collision-resistant, used everywhere
  — `Library_Resolution.md` §7a), so a remote serving different bytes
  for the same revision fails the hash check. No algorithm work is
  needed for level-3; the verification layer is already adequate.
- **Scheme/host allowlist (design only — NOT implemented).** A board
  could restrict where libs may come from:
  ```toml
  [libraries.policy]
  allowed_sources = ["git:ssh://git.acme/*", "p4://depot/bhdl-libs/*"]
  ```
  A `source` outside the allowlist would be rejected before any helper
  runs — defending against a malicious `bhdl.toml` edit pulling from
  elsewhere. There is no such manifest field today
  (`ProjectManifest` in `library.rs` has no policy table); this stays
  part of the stage-3 hardening (§9).
- **No arbitrary code at resolve time beyond the helper** the user chose
  to install and name on PATH.

## 6. Offline / CI

- `--offline`: never spawn a helper; every `source` dep must be
  satisfiable from cache (or vendored). Errors loudly otherwise.
  **Known gap:** the mechanism exists in the library
  (`source::FetchOptions { offline, resolver_dirs }` +
  `LibraryResolver::with_fetch_options`, exercised by tests) but is
  NOT reachable from the CLI — `bhdl-cli` never calls
  `with_fetch_options`, so there is no `--offline` flag and no way to
  configure resolver dirs today; helpers are found on `PATH` only, and
  builds are implicitly online (though a warm cache still means no
  helper is spawned).
- `--locked` (exists) + `--offline` = hermetic CI: build entirely from
  the committed lock + warm cache, no network, fail on any drift.
- A `bhdl fetch` command (pre-warm the cache for all locked deps) would
  let CI separate the network phase from the build phase — not built.

## 7. Core vs extension split

| Concern | Where |
|---|---|
| `source`/`rev` schema in manifest + lock | **core** |
| content-addressed cache + hash verify | **core** |
| helper discovery + JSON stdin/stdout protocol | **core** |
| `--offline`, `--locked`, allowlist policy, `bhdl fetch` | **core** |
| built-in `git` scheme | **core** (optional, §8) |
| `bhdl-source-p4`, `-svn`, `-<internal>` executables | **company extension** (~tens of lines) |

The core never grows a VCS dependency; adding Perforce support is a
script on PATH, shipped and maintained by whoever uses Perforce.

## 8. Decisions (mostly settled in code)

*(Hash algorithm — formerly the first decision — is settled: the lock
uses **sha256** everywhere, already shipped. No level-3 work needed
there.)*

1. **Helper protocol shape — DECIDED: JSON-on-stdin/stdout.** The
   shipped invocation (`source::run_helper`) writes the JSON request
   (`protocol: 1, locator, rev, dest, offline`) to the helper's stdin
   and reads an optional JSON reply (`ok` + `message`, §3) — room for
   auth hints, sparse-checkout, depth, without arg churn.
2. **Built-in git, or git-as-helper — STILL OPEN.** Today every scheme,
   git included, is an external `bhdl-source-<scheme>` helper; no
   built-in ships. *Lean: add a built-in git for the common case.*
3. **Cache location — DECIDED: global.** `$BHDL_CACHE/sources` if set,
   else `~/.cache/bhdl/sources`, else a process-temp fallback
   (`source::cache_root`). No project-local `.bhdl/cache`.
4. **Mutable-rev policy — DECIDED: warn.** `SourceSpec::rev_looks_mutable`
   flags `main`/`master`/`head`/`latest`/`trunk`/`tip`; the resolver
   logs a warning and proceeds (the content-hash still catches the
   resulting drift). Only an *empty* rev is a hard error.

## 9. Staging

1. ✅ **Schema + cache + verify (no fetch).** `source`/`rev` in
   Dependency + lock; keyed cache; sha256 hash w/ algorithm tag;
   resolution path that reads from a pre-populated cache. Levels 1–2
   keep working untouched.
2. ✅ (mostly) **Helper protocol.** Discovery (resolver dirs, then
   `PATH`), JSON invocation, end-to-end fetch→cache→verify→reuse.
   The **built-in git** scheme is the missing piece of this stage.
3. ⏳ **Hardening.** CLI `--offline` / resolver-dir wiring (the
   library-side `FetchOptions` exists, §6), `bhdl fetch`, allowlist
   policy. (The mutable-rev *warning* already ships.)

Tests (`bhdl-synthesizer/tests/source_fetch.rs` + `source.rs` unit
tests): a fixture helper that copies a known tree; fetch→cache→reuse,
offline-from-cache, spec parsing, mutable-rev flagging,
deterministic per-scheme cache keys.

## 10. Relationship to what's shipped

Levels 1–2 (`Library_Resolution.md` §7c) already give VCS-agnostic,
content-verified, reproducible archival with **no code**. Level-3 only
adds *toolchain-driven fetch* for shops that prefer it over
sync-then-`path=`. It is purely additive: a board with no `source` deps
never touches any of this.
