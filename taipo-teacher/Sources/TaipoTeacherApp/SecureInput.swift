import Carbon
import Foundation

/// Whether macOS has secure keyboard entry turned on.
///
/// A password field asks the system for secure event input, which is how a keyboard
/// remapper or a logger is meant to know to stop.  This app is a logger -- the chord codes
/// *are* the letters, and no redaction keeps the data useful -- so this is the difference
/// between a training tool and a password capture.
///
/// # What it covers
///
/// Password fields in native apps, `sudo` and `ssh` prompts in Terminal and iTerm, the
/// login window, and the lock screen.
///
/// # What it does not
///
/// A web page's password field in a browser that does not ask for secure input.  Safari
/// does; Chrome and Firefox generally do not.  That gap is real, it cannot be closed from
/// here, and the honest answer is the manual pause in the menu.
enum SecureInput {
    static var isEnabled: Bool { IsSecureEventInputEnabled() }
}
