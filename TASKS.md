# Steno keyboard firmware tasks

## Analysis

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
  - [ ] LED framework.
    - [ ] Overall LED framewook, hooked into Layout
    - [ ] Backend for LED framework,
  - [ ] `get_board_info()`
