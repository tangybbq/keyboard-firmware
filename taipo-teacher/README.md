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
- **The gap**: a web page's password field in a browser that does not ask for secure input.
  Safari does; Chrome and Firefox generally do not.  Use **Pause** in the menu for those.
- Logs are plain text under `~/Library/Application Support/TaipoTeacher/logs`, one file a
  day.  Nothing is sent anywhere.

### Checking that the pause works

Without typing a real password:

```
sudo -v          # then look at the menu bar, and Ctrl-C at the prompt
```

The icon changes and the menu says "Paused — password field" while the prompt is waiting.
`ssh` to anything that asks for a password does the same.
