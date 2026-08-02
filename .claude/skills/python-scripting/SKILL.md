---
name: python-scripting
description: >-
  Writing and reviewing expert, idiomatic Python — typed, tested, and tooled
  with ruff + mypy + pytest. Covers CLIs (argparse/click), pathlib and
  subprocess, dataclasses vs Pydantic, error handling and logging, packaging
  with pyproject, virtualenvs, async, and the correctness/performance gotchas
  that actually bite. Load when writing or reviewing any *.py, a CLI, a test
  module, or a pyproject.toml.
---

# Expert Python

House style: **type-annotated**, **`ruff` + `mypy --strict` clean**, tested with
**`pytest`**. Target the repo's declared `requires-python`; assume ≥3.11 unless
told otherwise (that's when `tomllib`, `Self`, `ExceptionGroup`, and cheap
`X | None` unions all exist). Prefer the standard library; add a dependency only
when it earns its place.

## Toolchain (the non-negotiables)

- **Format + lint with `ruff`** — it replaces black, isort, flake8, pyupgrade,
  and most plugins. `ruff format` then `ruff check --fix`. Configure in
  `pyproject.toml` under `[tool.ruff]`; enable at least `E,F,I,UP,B,SIM,RUF`.
- **Type-check with `mypy` (or `pyright`)** in strict mode. Types are only worth
  writing if a checker enforces them — untyped `def`s pass silently otherwise.
- **Test with `pytest`**: plain `assert`, fixtures over setUp, `tmp_path` for
  filesystem work, `monkeypatch` for env/attrs, `pytest.raises` for errors,
  `@pytest.mark.parametrize` instead of loops. `pytest.ini`/`pyproject` sets
  `addopts = "-q --strict-markers"`.
- **One command to gate CI**: `ruff format --check . && ruff check . && mypy . && pytest`.

## Typing that pulls its weight

- Annotate every public signature; let inference handle locals. Modern syntax:
  `list[str]`, `dict[str, int]`, `str | None` — no `typing.List`/`Optional`
  needed on 3.10+.
- `from __future__ import annotations` (or 3.14+ deferred eval) lets you
  reference not-yet-defined names and keeps annotations as strings — cheap and
  forward-compatible.
- Reach for `Protocol` (structural typing) over ABCs when you just need "has
  these methods". `TypedDict` for structured dicts (JSON payloads), `Literal`
  for fixed string sets, `Final` for constants, `assert_never()` in the `else`
  of an exhaustive match to make missing cases a **type error**.
- Narrow with `TypeGuard`/`TypeIs` rather than casting. Avoid `Any`; prefer
  `object` + narrowing, or a `cast()` you can point at and justify.
- `@overload` when the return type genuinely depends on argument types.

## Idioms that separate expert from adequate

- **`pathlib.Path`, not `os.path`** — `p.read_text()`, `p / "sub"`, `p.glob()`,
  `p.exists()`. Strings are for display, not path math.
- **Comprehensions and generators** over manual `append` loops; a generator
  (`(x for x in …)`) streams without materializing. But a comprehension purely
  for side effects is an anti-pattern — write the `for` loop.
- **`enumerate`, `zip`, `itertools`** (`chain`, `groupby`, `pairwise`,
  `batched` on 3.12+) instead of index bookkeeping.
- **`dataclasses`** for plain data (`@dataclass(frozen=True, slots=True)` for
  immutable, hashable, memory-lean records). Use **Pydantic** only when you need
  *validation/coercion at the boundary* (parsing external JSON/env/config).
- **`collections`**: `defaultdict`, `Counter`, `deque` (O(1) ends). **Never**
  use a mutable default argument (`def f(x=[])`) — it's shared across calls; use
  `None` + `x = x or []` (or `x if x is not None else []` when `[]` is falsy-OK).
- **`match`** for structural dispatch on shape (tuples, classes with
  `__match_args__`), not as a glorified `if` chain.
- **f-strings**; `f"{value!r}"` for debug repr, `f"{x=}"` for
  quick "name=value" traces, `f"{n:_}"`/`f"{ratio:.1%}"` for formatting.

## Errors, resources, logging

- **Catch narrowly.** Bare `except:` and blanket `except Exception` swallow
  `KeyboardInterrupt`/bugs — catch the specific exception you can handle. Re-raise
  with `raise NewError(...) from err` to preserve the chain; use `raise ... from
  None` only to deliberately hide an implementation-detail cause.
- **Custom exception hierarchy**: one base `class AppError(Exception)` per
  package so callers can `except AppError`. `ExceptionGroup` + `except*` for
  concurrent failures (async/`TaskGroup`).
- **`with` for every resource** — files, locks, sockets, DB sessions.
  `contextlib`: `@contextmanager` for lightweight ones, `ExitStack` for a
  dynamic number, `suppress(FileNotFoundError)` instead of try/except/pass.
- **Log, don't print, in libraries**: `logger = logging.getLogger(__name__)`;
  never `logging.basicConfig()` at import time (that's the *application's* call).
  Use `logger.exception()` inside an `except` to capture the traceback; pass args
  (`logger.info("x=%s", x)`) rather than pre-formatting.
- **EAFP over LBYL** where it reads cleaner (`try: d[k] except KeyError`) —
  Pythonic and race-free vs check-then-act.

## CLIs and subprocess

- **`argparse`** for stdlib-only tools; **`click`** or **`typer`** when you want
  decorators, nested commands, and completion. Keep `main(argv=None)` testable
  and return an exit code; guard with `if __name__ == "__main__": raise
  SystemExit(main())`.
- **`subprocess.run([...], check=True, capture_output=True, text=True)`** —
  **always a list, never `shell=True`** with interpolated input (shell
  injection). `check=True` turns non-zero into `CalledProcessError`. Set
  `timeout=` for anything that can hang.
- Read config with **`tomllib`** (stdlib, read-only, 3.11+); write with a
  third-party `tomli-w` if needed. Env via `os.environ` with explicit defaults
  and coercion.

## Concurrency — pick the right axis

- **I/O-bound, many tasks:** `asyncio` — `async def`, `await`, and
  `asyncio.TaskGroup()` (3.11+) for structured concurrency (child failures
  cancel siblings and surface as an `ExceptionGroup`). Don't call blocking code
  in a coroutine; offload with `asyncio.to_thread`.
- **I/O-bound, simple:** `concurrent.futures.ThreadPoolExecutor`.
- **CPU-bound:** `ProcessPoolExecutor` — the GIL means threads won't parallelize
  pure-Python compute (until/unless free-threaded 3.13+ is in play).
- Never share mutable state across threads without a `Lock`/`Queue`.

## Packaging & layout

- **`pyproject.toml` is the single source of truth** (PEP 621
  `[project]` metadata + build backend). No `setup.py`/`setup.cfg` for new work.
- **`src/` layout** (`src/pkg/…`) so tests run against the installed package, not
  the working copy — catches missing-data-file and import bugs early.
- Isolated envs: a `.venv` per project (`python -m venv`), or **`uv`** for fast
  installs/locking. Pin with a lockfile; keep runtime deps and dev deps
  (`[project.optional-dependencies]` / dependency groups) separate.
- Expose console commands via `[project.scripts]`, not ad-hoc shebang scripts on
  PATH.

## Gotchas that have burned people

- **Mutable default args** (above) — the #1 silent bug.
- **Late binding in closures/loops**: `[lambda: i for i in range(3)]` all return
  `2`. Bind now with a default arg: `lambda i=i: i`.
- **`is` vs `==`**: `is` only for `None`/singletons; identity for small ints and
  interned strings is a CPython accident, not a contract.
- **Floating point**: `0.1 + 0.2 != 0.3`. Money → `decimal.Decimal`; comparisons
  → `math.isclose`.
- **Copies**: assignment aliases; `list(x)`/`x.copy()` is shallow;
  `copy.deepcopy` for nested. Slicing copies lists.
- **Modifying a list/dict while iterating** it → skipped items or `RuntimeError`;
  iterate a copy or build a new collection.
- **Truthiness traps**: `if x:` is false for `0`, `""`, `[]`, `None` alike —
  when the distinction matters (e.g. "arg omitted" vs "arg is `0`"), test
  `if x is None:`.
- **`sys.path`/import cycles**: circular imports usually mean a module wants
  splitting; a local (function-level) import can break the cycle as a last
  resort.

## Reviewing Python — what to look for first

Untyped or `Any`-typed public API; bare/broad `except`; mutable default args;
resources opened without `with`; `shell=True` or string-built SQL (injection);
`print` in library code; missing/incorrect `__eq__`/`__hash__` on value types;
tests that assert nothing or over-mock; O(n²) membership tests on lists that
should be `set`s. Confirm the change ships with a `pytest` case that fails
without it.
