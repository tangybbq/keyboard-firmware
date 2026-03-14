# bbq-steno-macros Design

## Purpose

`bbq-steno-macros` provides a single procedural macro, `stroke!()`, that converts textual steno notation into `bbq_steno::stroke::Stroke` values at compile time.

Its role is narrow but important:

- let code use readable stroke literals such as `stroke!("ST")`
- validate those literals during compilation
- avoid runtime parsing cost in firmware and supporting tools

## Scope

The crate is intentionally minimal.

- One proc macro: `stroke`
- No runtime API beyond the expanded tokens
- No independent steno parser logic of its own

All parsing and semantic validation come from `bbq-steno::stroke::Stroke::from_text()`.

## Architecture

The macro implementation is a thin compile-time adapter around the runtime parser.

Flow:

1. Parse macro input as a Rust string literal (`syn::LitStr`).
2. Call `Stroke::from_text()` from `bbq-steno`.
3. If parsing fails, emit a compile error attached to the literal span.
4. If parsing succeeds, extract the raw `u32` encoding.
5. Expand to `::bbq_steno::stroke::Stroke::from_raw(<constant>)`.

The key design point is that the macro does not reimplement the stroke grammar. It reuses the canonical parser from `bbq-steno` so compile-time and runtime parsing stay aligned.

## Expansion Contract

`stroke!("ST")` expands to code equivalent to:

```rust
::bbq_steno::stroke::Stroke::from_raw(<encoded_u32>)
```

That means:

- the expanded value is a normal `Stroke`
- call sites do not depend on proc-macro-only types
- generated code is small and deterministic
- the parser cost is paid at compile time, not at runtime

Using `from_raw()` also avoids embedding parsing logic in the target binary.

## Input Model

The macro accepts exactly one string literal.

Examples of intended usage:

- `stroke!("S")`
- `stroke!("-G")`
- `stroke!("RA*U")`
- `stroke!("")`

Because input is parsed as `LitStr`, the macro does not accept arbitrary expressions, constants, or concatenated tokens. This is a deliberate simplification:

- syntax is easy to validate
- diagnostics can point directly at the literal
- expansion remains deterministic

## Error Handling

If the string is not valid steno syntax under `Stroke::from_text()`, the macro emits a normal Rust compile error via `syn::Error`.

Important properties:

- errors are surfaced during compilation, not at runtime
- the error is attached to the source span of the literal
- accepted syntax is exactly whatever `bbq-steno` currently accepts

As a result, any parser behavior change in `bbq-steno` also changes the accepted `stroke!()` language.

## Dependency Design

`bbq-steno-macros` depends on `bbq-steno`, not the other way around.

This makes `bbq-steno` the source of truth for:

- textual stroke grammar
- bit encoding
- conversion from text to raw stroke values

The tradeoff is that proc-macro compilation pulls in `bbq-steno` during the host build. That is acceptable here because the macro exists specifically to compile constants from the canonical implementation.

## Relationship to the Rest of the Repo

Typical call sites fall into three categories:

- firmware code that wants readable stroke constants
- tests and tooling that build dictionaries or compare stroke sequences
- constant-generation utilities such as `bbq-consts`

This crate therefore sits at the build-time boundary between human-authored source and the compact `Stroke` representation used everywhere else.

## Benefits

### Compile-time validation

Bad stroke literals fail the build immediately instead of producing runtime errors or silent wrong constants.

### Readability

`stroke!("STKPWHR")` is substantially easier to review than a raw integer bitmask.

### Single source of truth

The same parser logic is used for:

- runtime string parsing in `bbq-steno`
- compile-time literal parsing in `bbq-steno-macros`

That reduces the risk of grammar drift.

### Zero runtime parsing cost

Firmware and tools receive pre-encoded `Stroke` constants.

## Constraints and Tradeoffs

### Proc-macro overhead

Every `stroke!()` use is processed by the proc-macro system during compilation. That is usually acceptable, but it is more expensive than hard-coded numeric constants.

### Canonical-parser coupling

The macro inherits all quirks of `Stroke::from_text()`. For example, if the parser is intentionally strict about hyphen placement, the macro is strict in exactly the same way.

### Path coupling in expansion

Expansion uses the absolute path `::bbq_steno::stroke::Stroke::from_raw(...)`. That assumes the consuming crate refers to the library as `bbq_steno`, which is the normal Cargo crate-name mapping for `bbq-steno`.

## Why a Proc Macro Instead of `const fn`

The current parser in `bbq-steno` is not implemented as a `const fn`, and compile-time diagnostics from procedural macros are clearer than forcing users to encode strokes manually.

The proc-macro approach gives:

- good source-located errors
- no duplicate parser implementation
- no need to maintain handwritten bit encodings

## Testing Status

This crate currently has no dedicated tests of its own.

Confidence comes indirectly from:

- `bbq-steno` stroke parsing tests
- successful compilation of many `stroke!()` call sites across the repo

A useful future addition would be a `trybuild` test suite covering:

- valid literal expansion
- invalid literal diagnostics
- edge cases like empty strokes and number-bar forms

## Open Questions

### Dependency layering

The current layering is simple and correct, but it means macro compilation always depends on the full `bbq-steno` crate. If host-build cost becomes a problem, the parser could eventually be split into a smaller shared crate. There is no evidence this is necessary today.

### Public surface

The crate exposes exactly one macro. That is appropriate now. If more compile-time steno helpers are added later, this crate should stay tightly focused on literal-to-constant conversions rather than becoming a general-purpose codegen crate.
