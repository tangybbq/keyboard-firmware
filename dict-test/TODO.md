# Tasks for dictionary translation.

- [ ] Multiple dictionary support. Allow the dictionary conversion and loading
      code to handle multiple dictionaries. This will make it possible to turn
      the dictionaries on and off individually.
- [ ] Coherent states. Instead of several ad-hoc variables for the states,
      encode them better, and make how they are encoded in the dictionary
      entries more consistent.
      - Cap next
      - Lower next
      - Don't space
      - Force space (overrides "Don't space", for symbols)
      - Stitch: Don't space if previous entry was also a Stitch
- [ ] Implement retro changes. These really only need to see the translation
      text and insert a typing change, which will allow it to easily be undone.
- [ ] Implement Modified Emily's Symbols in rust.
- [ ] Taipo raw mode
- [ ] State-only strokes should preserve state from prior.  For example,
      Capitalizing the first word should still suppress the space.
- [ ] Ortho rules.  The ortho rules for Phoenix are simple, but still need to be
      implemented.
- [ ] Implement retro space adjustments
- [ ] Retro capitalization
- [ ] Retro currenty
- [ ] Make PHU/TAL/-BG "metallic" translate correctly.  It needs to use the
      earlier translation, and not mu + talc.

# Known bugs

- [*] These words do not seem to lookup correctly, despite having a full entry.
      - Clifton
      - righteous
      - squeamish
      - aquamarine
      - obnoxious
      - consensus
      - ametalic ??
      - catatonic
- [ ] Words where multiple steps of correction results in too much being deleted.
      - "An academic"


