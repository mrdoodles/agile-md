---
name: rust-minijinja
description: >-
  Templating in Rust with MiniJinja (the Jinja2-compatible engine) — Environment
  setup and template loading, the context!/Serialize bridge, custom
  filters/tests/functions, auto-escaping and undefined behaviour, inheritance
  and includes, dynamic objects, embedding templates in the binary, autoreload
  in dev, error reporting, sandboxing untrusted templates, and the gotchas that
  silently render the wrong thing. Load when adding or reviewing any *.jinja /
  *.html.j2 template, a `minijinja::Environment`, a custom filter, or any Rust
  code that renders text from data. General Rust style lives in the
  rust-programming skill.
---

# MiniJinja in Rust

**MiniJinja** (`minijinja`, 2.x) is Armin Ronacher's Jinja2-compatible template
engine: pure Rust, no unsafe, minimal deps, and syntax that matches Jinja2/Django
closely enough that existing templates and docs mostly transfer. Reach for it for
HTML pages, emails, config/codegen, release notes, SQL fragments, and CLI output —
anywhere the alternative is `format!` soup.

Rule of thumb: **one `Environment`, built once, shared for the process life.** It
owns the parsed template cache, filters, globals and escaping policy; rendering
takes `&self`, so an `Arc<Environment<'static>>` in app state is the shape you want.

## Setup

```toml
[dependencies]
minijinja = { version = "2", features = ["loader"] }   # builtins/macros are default
serde = { version = "1", features = ["derive"] }
```

```rust
use minijinja::{Environment, context, path_loader};

fn build_env() -> Environment<'static> {
    let mut env = Environment::new();          // ::empty() = no builtin filters/tests
    env.set_loader(path_loader("templates"));  // lazy, on-demand, cached after first load
    env.add_filter("shout", |s: &str| s.to_uppercase());
    env.add_global("site_name", "agile-md");
    env
}

let env = build_env();
let tmpl = env.get_template("page.html")?;
let html = tmpl.render(context! { title => "Board", items => &items })?;
```

- `add_template(name, source)` borrows a `&'source str` (great with
  `include_str!`); `add_template_owned` takes owned strings for runtime-built
  sources. `set_loader` is lazier and better for a template directory.
- One-offs: `env.render_str(source, ctx)?`, or the `render!` macro. Don't use
  these in a loop — they reparse every call.
- `env.compile_expression("score > 10 and active")?.eval(ctx)?` evaluates a
  single expression: a tidy way to make rules/filters user-configurable without
  a whole template.

## Getting data in

- **`context!` macro** for ad-hoc maps: `context! { user, count => 3, ..extra }`
  (field-init shorthand and `..` merge both work).
- **Anything `Serialize`** becomes a value: pass a struct/enum directly, or
  `Value::from_serialize(&data)`. Field names follow serde attributes, so
  `#[serde(rename_all = "camelCase")]` changes what the template must say.
- `Value` is cheap to clone (refcounted internally) — pass it around freely.
- **Dynamic/lazy data**: implement `Object` (`Debug + Send + Sync + 'static`) and
  use `Value::from_object` when you want the template to reach into something
  computed on demand (a DB handle, a big lazily-formatted collection) instead of
  serializing everything up front.

## Filters, tests, functions

```rust
use minijinja::{Error, ErrorKind, State, Value};

env.add_filter("slug", |s: &str| s.to_lowercase().replace(' ', "-"));
env.add_test("done", |s: &str| s == "done");
env.add_function("now", || chrono::Utc::now().to_rfc3339());

// Take &State first when you need the environment, template name, or lookups.
fn render_partial(state: &State, name: &str) -> Result<String, Error> {
    state.env().get_template(name)?.render(context! {})
}
```

- Arguments convert automatically (`&str`, `String`, `i64`, `Value`, `Option<T>`
  for optional args, `Rest<T>` for varargs, `Kwargs` for keyword args); return
  `T` or `Result<T, Error>`. Build errors with
  `Error::new(ErrorKind::InvalidOperation, "…")`.
- Filters are also usable as blocks: `{% filter upper %}…{% endfilter %}`.
- **`minijinja-contrib`** adds the batteries Jinja2 users expect —
  `datetimeformat`, `timeformat`, `pluralize`, `filesizeformat`, `wordwrap`,
  `truncate`, `randrange` — via `minijinja_contrib::add_to_environment(&mut env)`.
  Check there before hand-rolling a filter.

## Template syntax worth knowing

`{{ expr }}` output · `{% stmt %}` logic · `{# comment #}` · `{{- -}}` / `{%- -%}`
strip surrounding whitespace.

- **Inheritance**: `{% extends "base.html" %}` + `{% block content %}…{% endblock %}`,
  `{{ super() }}` to call the parent block. This is the primary structuring tool —
  prefer it to stitching strings in Rust.
- **Composition**: `{% include "partial.html" %}` (inherits the current context),
  `{% import "macros.html" as m %}` then `{{ m.badge(task) }}`, and
  `{% macro badge(t) %}…{% endmacro %}` for reusable snippets.
- **Loops**: `{% for t in tasks %}…{% else %}nothing{% endfor %}`, with
  `loop.index`, `loop.index0`, `loop.first`, `loop.last`, `loop.length`,
  `loop.cycle("a","b")`; `{% for … recursive %}` + `{{ loop(child) }}` for trees.
- **Bindings**: `{% set total = items | length %}`, `{% with x = f() %}…{% endwith %}`,
  `{% raw %}` to emit literal braces (essential when templating templates).
- **Everyday filters**: `default("-")`, `join(", ")`, `length`, `sort`, `reverse`,
  `map(attribute="name")`, `selectattr("done")`, `batch(3)`, `indent(4)`,
  `trim`, `title`, `tojson`, `escape`/`e`, `safe`, `int`, `float`, `first`, `last`.
- Missing keys give *undefined*, not an error, by default — see below.

## Auto-escaping (get this right or you ship XSS)

- Default policy keys off the **template name's extension**: `.html`, `.htm`,
  `.xml` escape; everything else doesn't. So `add_template("email", src)` renders
  **unescaped** — name it `email.html`, or set the policy explicitly:

```rust
use minijinja::AutoEscape;
env.set_auto_escape_callback(|name| match name.rsplit('.').next() {
    Some("html" | "htm" | "xml" | "j2") => AutoEscape::Html,
    Some("json") => AutoEscape::Json,
    _ => AutoEscape::None,
});
```

- `|safe` marks a string as pre-escaped — only ever apply it to markup *you*
  produced, never to user input. Values returned from filters are escaped as
  normal strings unless you return `Value::from_safe_string`.
- For non-HTML output (SQL, shell, YAML) auto-escaping does nothing useful:
  escape at the boundary with a real quoting function, exposed as a filter.

## Undefined behaviour, whitespace, and other config

```rust
use minijinja::UndefinedBehavior;
env.set_undefined_behavior(UndefinedBehavior::Strict); // missing var => error
env.set_trim_blocks(true);          // drop the newline after a block tag
env.set_lstrip_blocks(true);        // strip leading whitespace before a block tag
env.set_keep_trailing_newline(true);// keep the file's final \n (dropped by default)
```

- **Default is `Lenient`: an undefined variable renders as empty string.** That
  turns a typo into a silently blank page. Use `Strict` (at minimum in tests and
  CI) and be deliberate with `| default(…)`. `Chainable` sits in between,
  allowing `a.b.c` to walk through undefined.
- `custom_syntax` feature + `SyntaxConfig` when `{{ }}` collides with the output
  language (e.g. templating Jinja, Vue, or Go templates).
- Feature flags to know: `loader` (path loading), `json`/`urlencode` (extra
  filters), `preserve_order` (insertion-ordered maps), `debug` (rich errors, on
  by default), `fuel`, `custom_syntax`, `speedups`.

## Errors

`minijinja::Error` carries `kind()`, template name, and line. **`{}` prints only
the summary** — for a useful report print the alternate form and walk the chain:

```rust
Err(err) => {
    eprintln!("{err:#}");                       // includes debug info / source span
    let mut e: &dyn std::error::Error = &err;
    while let Some(next) = e.source() {
        eprintln!("caused by: {next:#}");
        e = next;
    }
}
```

An error raised inside a custom filter surfaces as the *source* of the render
error, so dropping the chain (a bare `.to_string()`, or `anyhow` without
`{:#}`) is how people end up with "invalid operation" and no location.

## Shipping templates

- **Development**: `minijinja-autoreload`'s `AutoReloader` rebuilds the
  environment when a watched path changes — `reloader.acquire_env()?` per
  request, no restart. Keep it behind `#[cfg(debug_assertions)]` or a feature.
- **Release**: embed templates so the binary is self-contained — `include_str!`
  + `add_template` for a handful, `minijinja-embed` (`embed_templates!` in
  `build.rs`, `load_templates!` at startup) for a whole directory. Then a
  missing template is a build-time problem, not a 500 in production.
- **Test the templates**: render each with `UndefinedBehavior::Strict` and a
  representative context in a unit test, and snapshot the output with `insta`.
  Template bugs don't fail compilation — tests are your only type system here.
- `minijinja-cli` renders a template against JSON/YAML/TOML from the shell:
  handy for release notes, scaffolding, and debugging a context without writing
  a program.

## Untrusted templates

Templates are code. If users supply them: build from `Environment::empty()` and
add back only the filters/globals you intend; never expose an `Object` that can
reach the filesystem, env vars, or network; enable the `fuel` feature and
`env.set_fuel(Some(…))` to bound runaway loops; cap output size and render on a
thread you can abandon. MiniJinja has no `eval`/import-from-Rust escape hatch by
design, so the attack surface is whatever *you* put in the environment.

## Gotchas

- **A template name with no known extension is not escaped.** The most common
  MiniJinja XSS.
- **Lenient undefined hides typos** — `{{ tsak.title }}` renders nothing rather
  than failing.
- **Rebuilding the `Environment` per request** reparses everything; it's the
  usual cause of "MiniJinja is slow".
- **`add_template` borrows** — a `String` built at runtime needs
  `add_template_owned`, or you'll fight lifetimes for no reason.
- **serde rename attributes change template field names**; a `#[serde(skip)]`
  field is simply undefined in the template.
- **Trailing newlines are stripped by default** — matters for generated files
  where the final `\n` is significant (POSIX text files, git-diff noise).
- **`{% include %}` inherits context, `{% import %}` does not** (imported macros
  don't see the caller's variables unless you pass them or use
  `{% import … with context %}`).
- **Whitespace control is per-tag**: prefer `{%- -%}` on the offending tag over
  global `trim_blocks` when only one spot misbehaves — the global switches change
  every template at once.
