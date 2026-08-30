// Watch macOS's secure keyboard entry flag, and report when it changes.
//
// The app pauses recording whenever this is on, and which situations turn it on is not
// something to take on trust: it depends on the app that owns the text field, and the
// answers are surprising.  Terminal asserts it for a password prompt; the same prompt
// inside tmux does not, because Terminal only sees tmux's tty.  iTerm2 may differ again.
//
// Run this, then go and do the thing you want to know about.
import Carbon
import Foundation

// Unbuffered, so piping it to a file or watching it live both show changes as they
// happen rather than when the process ends.
setvbuf(stdout, nil, _IONBF, 0)

print("Watching secure keyboard entry.  Ctrl-C to stop.")
print("Try: a password prompt in a bare shell, the same inside tmux, a browser login,")
print("     locking the screen.  Anything that prints ON is covered; anything that")
print("     stays OFF is not.\n")

var last: Bool?
let started = Date()
while true {
    let now = IsSecureEventInputEnabled()
    if now != last {
        let stamp = String(format: "%7.1fs", Date().timeIntervalSince(started))
        print("\(stamp)  \(now ? "ON  — recording would pause" : "OFF — recording would run")")
        last = now
    }
    Thread.sleep(forTimeInterval: 0.1)
}
