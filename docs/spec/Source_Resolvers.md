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
  Perforce changelist number, a content digest. Mutable refs (`main`,
  `HEAD`, `latest`) are rejected with a warning (the content-hash would
  catch drift anyway, but we fail early).
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
{ "protocol": 1, "ok": true }
```

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

Content-addressed cache, default `~/.cache/bhdl/sources/` (override via
`$BHDL_CACHE` or a config key; project-local `.bhdl/cache` for hermetic
checkouts).

Resolving a `source` dependency:
1. If a lock entry exists and `cache/<hash>/` is present and re-hashes
   to the locked hash → use it. **No fetch, no helper, works offline.**
2. Else fetch: spawn `bhdl-source-<scheme>` into a scratch dir, hash the
   result, move it to `cache/<hash>/`.
3. Verify: against the lock (if locked) the fetched hash must equal the
   locked hash → mismatch is a hard error (`Content` drift — a remote
   served different bytes for the same rev, or a mutable rev moved). On
   a first build (no lock), record the hash into the lock.
4. Verify the library's `manifest.toml` version against the declared
   `version` (existing check).
5. Return the cache path; the import loader proceeds as for any root.

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
- **Scheme/host allowlist (optional).** A board may restrict where libs
  may come from:
  ```toml
  [libraries.policy]
  allowed_sources = ["git:ssh://git.acme/*", "p4://depot/bhdl-libs/*"]
  ```
  A `source` outside the allowlist is rejected before any helper runs —
  defends against a malicious `bhdl.toml` edit pulling from elsewhere.
- **No arbitrary code at resolve time beyond the helper** the user chose
  to install and name on PATH.

## 6. Offline / CI

- `--offline`: never spawn a helper; every `source` dep must be
  satisfiable from cache (or vendored). Errors loudly otherwise.
- `--locked` (exists) + `--offline` = hermetic CI: build entirely from
  the committed lock + warm cache, no network, fail on any drift.
- A `bhdl fetch` command (pre-warm the cache for all locked deps) lets
  CI separate the network phase from the build phase.

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

## 8. Open decisions (need a call before building)

*(Hash algorithm — formerly the first decision — is settled: the lock
uses **sha256** everywhere, already shipped. No level-3 work needed
there.)*

1. **Helper protocol shape.** JSON-on-stdin/stdout (proposed, extensible)
   vs plain CLI args (simpler). *Lean: JSON — room for auth hints,
   sparse-checkout, depth, without arg churn.*
2. **Built-in git, or git-as-helper.** Ship a `git` built-in for the
   common case vs treat every scheme uniformly as a helper. *Lean:
   built-in git + helper protocol for the rest.*
3. **Cache location.** Global `~/.cache/bhdl` (Cargo-style, cross-project
   reuse) vs project-local `.bhdl/cache` (hermetic). *Lean: global with a
   project-local override + `$BHDL_CACHE`.*
4. **Mutable-rev policy.** Warn vs hard-error on `main`/`HEAD`/`latest`.
   *Lean: warn (the content-hash still catches the resulting drift).*

## 9. Staging

1. **Schema + cache + verify (no fetch).** `source`/`rev` in
   Dependency + lock; content-addressed cache; sha256 hash w/ algorithm
   tag; resolution path that reads from a pre-populated cache. Testable
   by hand-filling the cache. Lets levels 1–2 keep working untouched.
2. **Helper protocol + built-in git.** Discovery, JSON invocation, the
   git built-in. End-to-end fetch→cache→verify→reuse.
3. **Hardening.** `--offline`, `bhdl fetch`, allowlist policy,
   mutable-rev check.

Tests: a fixture `bhdl-source-test` helper that copies a known tree;
assert fetch→cache→hash-verify→reuse, offline-from-cache, drift on a
tampered cache entry, allowlist rejection, version-mismatch.

## 10. Relationship to what's shipped

Levels 1–2 (`Library_Resolution.md` §7c) already give VCS-agnostic,
content-verified, reproducible archival with **no code**. Level-3 only
adds *toolchain-driven fetch* for shops that prefer it over
sync-then-`path=`. It is purely additive: a board with no `source` deps
never touches any of this.
