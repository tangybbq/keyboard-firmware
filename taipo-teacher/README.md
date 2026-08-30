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

### Checking that the pause works

Without typing a real password:

```
sudo -v          # in a BARE terminal, not tmux; watch the menu bar, then Ctrl-C
```

The icon changes and the menu says "Paused — password field" while the prompt is waiting.
Doing the same inside tmux will *not* pause it, which is the gap above, and is worth seeing
for yourself so the difference is concrete.
