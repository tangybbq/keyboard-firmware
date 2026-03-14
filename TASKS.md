# Steno keyboard firmware tasks

## Analysis

- [X] Produce a `DESIGN.md` file at the top of each crate describing the design and architecture of the
  code.
  - This should be of sufficient detail to reason about the code in that crate without necessarily
    having to load the source files from the crate.
  - It should be readable both to a human reader in the future, and to the Agent to assist with
    performing upcoming work.

- [ ] Cleanup
  - [X] Remove the old Translator code files, and references from DESIGN.md

- [ ] Tests
  - [ ] Write isolated tests for the Taipo keyboard implementation
    - Come up with a mechanism to build test cases
    - Test the various modifier key operations.
    - Test that key rollover between the two halves works correct
    - Write a test to perform 3 key rollover to demonstrate that it doesn't work. The test can then
      be disabled pending a fix. Three key rollover involves pressing a key on the left, another key
      on the right, and than an additional different key on the left. Currently, the third key will
      not register.

- [ ] Jolt
  - [X] Ensure the instruction on building 'jolt' are correct for the agent.
  - [ ] Add basic HID device support, with a C source file containing an initializer function that
    will be called from the Rust code, and an entry point to make a HID event present. As HID events
    can only be queued one at a time, this shouldn't worry about being usable yet, and we'll figure
    out timing in subsequent changes.
