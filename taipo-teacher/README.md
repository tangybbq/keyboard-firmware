# Taipo Teacher

The Mac half of `taipo-teacher.md`: it reads the keyboard's key event log, records it, and
drills against it.

```
scripts/bundle.sh --run
```

Menu bar only, no dock icon.  Recording runs whenever the app does.

## Privacy

**This records everything typed on the keyboard.**  The chord codes are the letters, so
there is no redaction that leaves the log useful; the protections are about *not recording*
rather than about obscuring what was recorded.

- The device records nothing until asked, and is told to stop when the app quits.
- Recording stops automatically whenever macOS turns on **secure keyboard entry**: password
  fields in native apps, `sudo` and `ssh` prompts, the login window, and the lock screen.
  The check happens before each fetch, so a keystroke typed into a password field is
  discarded from the keyboard's buffer rather than written.
- **The gaps**, which are real and worth knowing:
  - **A password prompt inside tmux or screen.**  Terminal asserts secure input from its own
    tty's state, and tmux owns that tty; `sudo` inside a tmux pane sets *its* pty to noecho,
    which Terminal never sees.  A bare terminal is covered; a multiplexed one is not.
  - A web page's password field in a browser that does not ask for secure input.  Safari
    does; Chrome and Firefox generally do not.
- For those, either **Pause** before typing, or **Discard recent typing** afterwards, which
  truncates the log back to where it stood and clears the keyboard's buffer.  Prevention only
  covers what the system tells us about, so there has to be a way to take something back.
- Logs are plain text under `~/Library/Application Support/TaipoTeacher/logs`, one file a
  day.  Nothing is sent anywhere.

### Finding out what is actually covered

Which situations turn secure input on depends on the app that owns the text field, and the
answers are surprising enough not to take on trust.  `secwatch` reports the flag as it
changes:

```
swift run secwatch
```

Then go and do the thing you want to know about — a password prompt in a bare shell, the
same prompt inside tmux, a browser login, locking the screen.  Anything that prints `ON` is
covered; anything that stays `OFF` is not, and wants **Pause** beforehand or **Discard
recent typing** afterwards.

Worth mapping this once for the terminal you actually use.  Terminal.app asserts secure
input for a password prompt and iTerm2 may not: iTerm2's *Secure Keyboard Entry* is a menu
toggle rather than something it turns on by itself.
