This is a simple utility for performing diagnostics on the dictionary.

It is a rust program that uses the ../bbq-steno package for dictionary lookup.  The main tool of
concern: `cargo run write`, waits for textual RAW steno (the keyboard in raw steno mode), and upon
getting each stroke, it prints what the translation should do.
