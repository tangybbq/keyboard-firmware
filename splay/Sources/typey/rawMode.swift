// Use rawmode terminal.
//
// Taken from:
// https://gist.github.com/dduan/272d8c20bb6521695bd04e290b489774
// But with many improvements:
// - Use tcflag_t instead of conditionalizing the flags
// - Have a <T> parameter for a return value
// - Pass a cooked parameter to the closure it can use to run code
//   cooked.

#if canImport(Darwin)
import Darwin
#elseif canImport(Glibc)
import Glibc
#else
#error("Unable to find usable Posix library.")
#endif

enum RawModeError: Error {
    case notATerminal
    case failedToGetTeriminalSetting
    case failedToSetTeriminalSetting
}

func runInRawMode<T>(_ task: ((() throws -> Void) throws -> Void) throws -> T) throws -> T {
    var originalTermSetting = termios()
    guard isatty(STDIN_FILENO) != 0 else {
        throw RawModeError.notATerminal
    }

    guard tcgetattr(STDIN_FILENO, &originalTermSetting) == 0 else {
        throw RawModeError.failedToGetTeriminalSetting
    }

    var raw = originalTermSetting
    raw.c_iflag &= ~tcflag_t(BRKINT | ICRNL | INPCK | ISTRIP | IXON)
    raw.c_oflag &= ~tcflag_t(OPOST)
    raw.c_cflag |= tcflag_t(CS8)
    raw.c_lflag &= ~tcflag_t(ECHO | ICANON | IEXTEN | ISIG)
    // TODO: Figure out how to not hard code these, but use VMIN, and
    // VTIME. The problem is that c_cc is a tuple.
    raw.c_cc.16 = 1
    raw.c_cc.17 = 0

    guard tcsetattr(STDIN_FILENO, TCSAFLUSH, &raw) == 0 else {
        throw RawModeError.failedToSetTeriminalSetting
    }

    defer {
        tcsetattr(STDIN_FILENO, TCSAFLUSH, &originalTermSetting)
    }

    // task can invoke 'cooked' to run some code back in cooked mode.
    func cooked(_ action: () throws -> Void) throws {
        guard tcsetattr(STDIN_FILENO, TCSAFLUSH, &originalTermSetting) == 0 else {
            throw RawModeError.failedToSetTeriminalSetting
        }
        // If this throws, go back to raw.
        defer {
            tcsetattr(STDIN_FILENO, TCSAFLUSH, &raw)
        }
        try action()
    }

    return try task(cooked)
}

// A single get character routine.
func readCharacter() -> Character? {
    let ch = getchar()
    // print("ch = \(ch)\r")
    if ch < 0 {
        return nil
    }
    return Character(Unicode.Scalar(UInt8(ch)))
}
