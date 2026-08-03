---
name: rust-programming
description: >-
  Writing and reviewing expert, idiomatic Rust — clippy-clean, tested, and
  designed around ownership rather than fighting it. Covers cargo/workspace
  layout, error handling (thiserror vs anyhow), traits, generics and lifetimes,
  iterators and collections, API design, async/tokio, unsafe, FFI, testing and
  benchmarking, and the borrow-checker/perf gotchas that actually bite. Load
  when writing or reviewing any *.rs, a Cargo.toml, a build.rs, or a Rust CLI.
  For templating with MiniJinja see the rust-minijinja skill.
---

# Expert Rust

House style: **edition 2024** (2021 if the crate hasn't migrated), `rustfmt`
default profile, **`cargo clippy --all-targets -- -D warnings` clean**, tested
with `cargo test`. `unsafe` is opt-in and justified in a comment; most crates
should carry `#![forbid(unsafe_code)]` in `lib.rs`. Prefer `std`; add a
dependency when it earns its place — every crate is a compile-time and
supply-chain cost.

## Toolchain (the non-negotiables)

- **`cargo fmt --check` + `cargo clippy --all-targets --all-features -- -D warnings`
  + `cargo test`** is the CI gate. Clippy is not optional lint noise: its
  `perf`, `correctness`, and `suspicious` groups catch real bugs.
- **Pin the toolchain** with `rust-toolchain.toml` (`channel`, `components =
  ["rustfmt", "clippy"]`) so local and CI agree. Declare `rust-version` (MSRV)
  in `Cargo.toml` and let `cargo msrv`/CI verify it.
- **`Cargo.lock` is committed for binaries**, and these days for libraries too
  (it only affects *your* CI, never downstream consumers).
- Useful extras: `cargo deny check` (licenses/advisories/dupes),
  `cargo nextest run` (faster, better test output), `cargo machete`/`udeps`
  (unused deps), `cargo miri test` (UB in `unsafe`), `cargo doc --open`.
- **Workspaces**: one `[workspace]` root, `[workspace.dependencies]` +
  `dep.workspace = true` in members so versions are declared once. Keep
  `resolver = "2"` (implicit in edition 2021+) so dev/build deps don't leak
  features into the normal graph.

## Ownership — design with it, not against it

- **Take what you need, no more**: `&str` over `&String`, `&[T]` over `&Vec<T>`,
  `&Path` over `&PathBuf`. Take by value (`String`, `T`) when you genuinely
  store it; `impl AsRef<Path>` / `impl Into<String>` at ergonomic boundaries
  only — a generic param in every signature bloats both API and codegen.
- **Return owned data from constructors, borrow in accessors.** A method
  returning `&T` ties the caller to your borrow; that's usually right, but is
  the reason a "just add a getter" can cascade into borrow errors.
- **`Clone` is a legitimate tool.** A `String` clone in a setup path is fine;
  cloning in a hot loop is not. Reach for `Rc<T>`/`Arc<T>` for shared ownership,
  `Cow<'_, str>` when you *usually* borrow and occasionally own.
- **If you're fighting the borrow checker, the data model is usually wrong.**
  The standard fixes: split the struct so disjoint fields borrow independently;
  index into a `Vec` instead of holding `&mut` across a loop; move shared state
  behind `Rc<RefCell<_>>`/`Arc<Mutex<_>>`; or restructure so the borrow ends
  before the next call (NLL ends borrows at last use, not end of scope).
- **Self-referential structs don't work.** Store indices/ids, or use an arena
  (`slotmap`, `typed-arena`) — not `Pin` gymnastics.
- **Interior mutability**: `Cell` (Copy, no borrow tracking), `RefCell`
  (runtime borrow panics — single-threaded), `Mutex`/`RwLock` (threads),
  `OnceLock`/`LazyLock` for lazily-initialised globals instead of `lazy_static`.

## Types and API design

- **Make illegal states unrepresentable.** Enums over boolean pairs; newtypes
  (`struct UserId(u64)`) over bare primitives; `NonZeroU32`, `NonEmpty`-style
  wrappers where the invariant is real. Parse at the boundary into a validated
  type — don't validate repeatedly downstream.
- **Enums + exhaustive `match`** are the workhorse. Avoid catch-all `_ =>` arms
  in your own crate: adding a variant should be a *compile error* everywhere it
  matters. Mark public enums/structs `#[non_exhaustive]` when you intend to add
  variants without a semver break.
- **Derive the standard set** where meaningful: `Debug` on nearly everything,
  plus `Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default`. Implement
  `Display` for user-facing text (never `Debug` for that), `FromStr` for
  parsing, `From`/`TryFrom` for conversion — then `?` and `.into()` work for
  free. Implementing `From` gives you `Into` automatically; never write both.
- **Traits**: use them for shared behaviour, not to mimic OO inheritance.
  Generics (`fn f<T: Trait>`) monomorphise — fast, bigger binary; `dyn Trait`
  behind `&`/`Box` — one copy, dynamic dispatch, and requires object safety.
  Default to generics for leaf functions, `dyn` for plugin-ish collections.
  Sealed traits (a private supertrait) prevent downstream impls when you need
  to keep the freedom to add methods.
- **Builders** for structs with many optional fields; `#[derive(Default)]` plus
  `..Default::default()` in struct-update syntax for the simple cases.
- **Public API hygiene**: `#![warn(missing_docs)]` on libraries, `#[must_use]`
  on pure functions returning a value that's pointless to discard, doc examples
  that run as tests, and re-export the crate's public surface from `lib.rs` so
  module paths are an implementation detail.

## Errors

- **Libraries: concrete error enums via `thiserror`.** One error type per crate
  (or per module for big crates), `#[from]` for wrapped sources, `#[error("…")]`
  for `Display`. Callers can `match` on your variants — a boxed opaque error
  takes that away.
- **Binaries: `anyhow`** (or `eyre`). `?` everywhere, `.context("reading config
  {path}")` at each layer so the final report reads like a trail. `fn main() ->
  anyhow::Result<()>` prints the chain and exits non-zero.
- **Never `unwrap()`/`expect()` on anything a user can influence.** `expect` is
  acceptable for genuine invariants — and then the message states *why it can't
  fail*, not what failed. Same for `panic!`: panics are for bugs, `Result` is
  for expected failure. In library code, avoid indexing (`v[i]`) and integer
  division that can panic; use `get()`, `checked_*`, `try_into()`.
- **`Option` combinators over nesting**: `map`, `and_then`, `unwrap_or_else`,
  `ok_or_else`, `filter`, `?`. `let … else { return … };` flattens the
  "extract or bail" shape without an `else` pyramid.
- Don't stringify errors (`.map_err(|e| e.to_string())`) — it destroys the
  source chain and any structured data.

## Iterators, collections, strings

- **Iterator chains over index loops**: `filter`, `map`, `find`, `position`,
  `take_while`, `zip`, `enumerate`, `flat_map`, `chunks`, `windows`, `fold`,
  `any`/`all`. Iterators are lazy — nothing runs until consumed, and
  `collect::<Result<Vec<_>, _>>()` short-circuits on the first error (a
  genuinely great trick).
- `collect()` into `String`, `HashMap`, `Result`, `Option` — the target type
  drives it; annotate with turbofish when inference can't see it.
- **Pick the right container**: `Vec` by default; `VecDeque` for ends;
  `HashMap`/`HashSet` for lookup (`BTreeMap` when you need ordering or range
  queries); `HashMap::entry().or_insert_with()` instead of contains-then-insert.
  Pre-`with_capacity` when the size is known.
- **Strings are not arrays of characters.** `String`/`&str` are UTF-8: indexing
  by byte range panics mid-codepoint; use `.chars()`, `.char_indices()`,
  `.split_whitespace()`, `.strip_prefix()/.strip_suffix()`, `.trim()`. `.len()`
  is bytes. Build with `push_str`/`write!` into a `String`, not `+` in a loop.
  Use `OsStr`/`Path` for filesystem paths — they aren't guaranteed UTF-8.
- Prefer `matches!(x, Pat)` to a two-arm match returning bools, and
  `if let … else if let …` chains to nested matches.

## CLIs, config, I/O

- **`clap` with the derive API** for anything non-trivial (subcommands, env
  fallbacks, `--help` generation, shell completions via `clap_complete`).
- Keep `fn main()` thin: parse args, call `run(args) -> Result<()>`, map to an
  exit code. That makes the logic testable without spawning a process.
- **Buffer your I/O**: `BufReader`/`BufWriter`; locking `stdout()` once outside
  a print loop is a large win. `println!` panics on a broken pipe — for tools
  meant to be piped into `head`, write to a locked handle and handle
  `ErrorKind::BrokenPipe` yourself.
- `serde` (+ `serde_json`/`toml`) for config; `#[serde(deny_unknown_fields)]`
  catches typo'd keys, `#[serde(default)]` for optional ones.
- **`std::process::Command`** takes program + args separately — there is no
  shell, so no injection, but also no globbing/pipes unless you invoke a shell
  deliberately. Always check `status.success()`.
- Logging: `tracing` (spans + structured fields, async-aware) or `log` + `env_logger`
  for simple tools. Libraries emit; the binary initialises the subscriber.

## Async

- **Only go async for I/O concurrency.** Async has real costs (`Send + 'static`
  bounds, coloured functions, harder borrows). Threads + channels are often the
  better answer for CPU work or a handful of tasks.
- **`tokio`** is the default runtime; pick features explicitly rather than
  `full` in libraries. Libraries should stay runtime-agnostic where practical.
- **Never block in an async fn** — no `std::thread::sleep`, no blocking file
  I/O, no long CPU loops. Use `tokio::time::sleep`, `tokio::task::spawn_blocking`,
  or `rayon` for parallel compute. One blocked task stalls a whole worker thread.
- **Don't hold a `std::sync::MutexGuard` across `.await`** (not `Send`, and it
  can deadlock). Either scope the lock so it drops first, or use
  `tokio::sync::Mutex` when the critical section really must await.
- Futures are inert until polled; `select!` cancels the losing branches at
  arbitrary await points, so anything half-done must be cancellation-safe.
  `JoinSet` for a dynamic fan-out, and *always* handle `JoinHandle` errors
  (a spawned task's panic is otherwise silent).

## Unsafe, FFI, and performance

- `unsafe` blocks are as small as possible, each with a `// SAFETY:` comment
  stating the invariant that makes it sound. Run `cargo miri test` over them.
  Prefer a well-audited crate (`bytemuck`, `zerocopy`) to hand-rolled transmutes.
- **Measure before optimising**: `criterion` for benches, `cargo flamegraph` /
  `samply` for profiles, and always benchmark `--release` (debug Rust is ~10-100×
  slower and tells you nothing).
- Cheap wins, in order: build in release with `lto = "thin"` and
  `codegen-units = 1`; remove needless allocations (`&str` over `String`,
  `with_capacity`, reuse buffers); avoid `collect()` in the middle of a chain;
  swap `HashMap`'s SipHash for `ahash`/`rustc-hash` when keys aren't
  adversarial; `rayon`'s `par_iter()` for embarrassingly parallel work.
- `#[inline]` only across crate boundaries where it measurably helps — inside a
  crate the optimiser already decides.

## Testing

- **Unit tests live in the file** under `#[cfg(test)] mod tests { use super::*; }`
  — they can exercise private items. **Integration tests in `tests/`** see only
  the public API (which is itself a useful design check).
- **Doc examples are tests**: `cargo test` runs them, so they can't rot. Use
  `# ` to hide setup lines and `no_run`/`ignore` sparingly.
- `assert_eq!` with a message, `#[should_panic(expected = "…")]` for panics,
  `-> Result<(), E>` test signatures so you can use `?`.
- Table-driven tests via a `for (input, want) in [...]` loop or `rstest`;
  `insta` for snapshot assertions; `proptest`/`quickcheck` for invariants;
  `tempfile::tempdir()` for filesystem work; `assert_cmd` + `predicates` for
  end-to-end CLI behaviour.
- Every bug fix ships with a test that fails without it.

## Gotchas that have burned people

- **Integer overflow panics in debug, wraps in release.** Use `checked_`,
  `saturating_`, or `wrapping_` explicitly when the behaviour matters. Enable
  `overflow-checks = true` in the release profile if correctness > speed.
- **`as` casts silently truncate** (`300u32 as u8 == 44`). Use `TryFrom`/
  `try_into()` for anything narrowing.
- **`sort_by(|a, b| a.partial_cmp(b).unwrap())` panics on `NaN`** — floats
  aren't `Ord`. Use `sort_by(f64::total_cmp)`.
- **`impl Drop` + moves**: you can't move a field out of a type that implements
  `Drop`; and drop order is declaration order for fields, reverse for locals.
  `std::mem::take`/`replace` is the escape hatch.
- **`Iterator::for_each` with `?` doesn't work** — use a `for` loop, or
  `try_for_each`.
- **Shadowing is a feature, not a bug**, but `let x = x.trim();` inside a long
  function hides type changes; keep shadowed names close together.
- **A `match` on `&Option<T>` binds by ref** thanks to default binding modes —
  fine, until you need ownership: then `opt.take()` or match on `opt` directly.
- **Trait method resolution**: an inherent method shadows a trait method of the
  same name; `Deref` makes `&Vec<T>` behave like `&[T]` and hides where a
  method actually came from. When in doubt, `Trait::method(&x)`.
- **Blanket `impl<T> Trait for T`** in a library is nearly always a mistake —
  it forecloses every other impl, and removing it is a breaking change.
- **Feature flags must be additive.** A feature that *changes* behaviour breaks
  under Cargo's feature unification when two dependents disagree.

## Reviewing Rust — what to look for first

`unwrap`/`expect`/indexing on fallible paths; `unsafe` without a `// SAFETY:`
note; error types that stringify or discard their source; blocking calls or
non-`Send` guards held across `.await`; `clone()` inside hot loops; `pub` items
that shouldn't be (a leaked internal type is a semver commitment); missing
`#[non_exhaustive]` on public enums that will grow; panicking `Drop` or `Ord`
impls; `HashMap` iteration order relied upon; tests that assert nothing.
Confirm the change is clippy-clean and ships with a failing-before test.
