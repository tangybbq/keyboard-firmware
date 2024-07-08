import Steno

import Foundation

// RTF CRE is a deeply under-specified format.  It is built around the
// RTF document format, with some somewhat randomly defined terms.
// We're really only interested in processing the dictionary entries,
// and don't care about the headers and such that make all of this
// work.

// At a basic level, RTF consists of a sequence of markup separated by
// plain text (which is similar to XML), but the markup itself can
// have content.  In this sense, it is actually a bit cleaner than
// XML.
//
// Markup is delineated by either `\name` or `{\name content)`.  The
// name is followed by an optional space, which is eaten, or can be
// directly followed by something that isn't a valid character in a
// name.
//
// Normally, newlines are not significant in RTF, but because of the
// weird way RTF/CRE encodes the stroke, we will assume that
// definitions end at a newline.  This will at least support the
// Phoenix Dictionary that comes out of Case CATalyst.

enum RTFError: Error {
    case fileError
    case multiplePush
    case invalidInput(char: Character)
    case unexpectedToken(token: Token)
}

enum Token: Comparable {
    case text(String)
    case star
    case open
    case close
    case newline
    case command(String)
    case eof
}

struct Definition {
    var strokes: [Stroke]
    var text: [Token]
}

fileprivate extension Character {
    var isNormalChar: Bool {
        get {
            switch self {
            case "\r", "\n", "\\", "{", "}":
                return false
            default:
                return true
            }
        }
    }

    var isIdentifierChar: Bool {
        get {
            isLetter || isNumber
        }
    }

    var escapable: Bool {
        get {
            switch self {
            case "\\", "{", "}", "~":
                return true
            default:
                return false
            }
        }
    }
}

/// Lexical analysis of the RTF file.
struct RTFLexer {
    let handle: FileHandle
    var bytes = Data().makeIterator()
    // Set when we've reached the end of the file.
    var done = false
    // We can push back a single character (used for lookahead)
    var pushed: Character? = nil

    init(path: String) throws {
        guard let handle = FileHandle(forReadingAtPath: path) else {
            throw RTFError.fileError
        }
        self.handle = handle
    }

    /// Return the next token from the stream.
    mutating func next() throws -> Token {
        guard let ch = try nextChar() else {
            return .eof
        }

        // Single character token characters.
        if ch.isNormalChar {
            var text = String(ch)

            while true {
                guard let ch2 = try nextChar() else {
                    done = true
                    break
                }
                if ch2.isNormalChar {
                    text.append(ch2)
                } else {
                    try pushChar(ch2)
                    break
                }
            }

            return .text(text)
        }

        if ch == "{" {
            return .open
        }
        if ch == "}" {
            return .close
        }
        if ch == "\n" {
            return .newline
        }
        if ch == "\r" {
            guard let ch2 = try nextChar() else {
                done = true
                return .newline
            }
            if ch2 != "\n" {
                try pushChar(ch2)
            }
            return .newline
        }
        if ch == "\\" {
            guard let ch2 = try nextChar() else {
                // File cannot end with backslash.
                throw RTFError.invalidInput(char: ch)
            }
            if ch2.escapable {
                return .text(String(ch2))
            }
            if ch2 == "*" {
                return .star
            }
            if ch2.isIdentifierChar {
                var text = String(ch2)

                while true {
                    guard let ch3 = try nextChar() else {
                        done = true
                        break
                    }
                    if ch3.isIdentifierChar {
                        text.append(ch3)
                    } else if ch3 == " " {
                        // Eat the optional space.
                        break
                    } else {
                        try pushChar(ch3)
                        break
                    }
                }

                return .command(text)
            }

            // Invalid character after backslash.
            throw RTFError.invalidInput(char: ch2)
        }
        throw RTFError.invalidInput(char: ch)
    }

    /// Return the next byte from the input stream, returning nil when
    /// the end of file is reached.
    mutating func nextByte() throws -> UInt8? {
        if done {
            return nil
        }

        if let ch = bytes.next() {
            return ch
        } else {
            // Iterator is empty, get more data.
            let buf = handle.readData(ofLength: 4096)
            if buf.isEmpty {
                done = true
                return nil
            }
            self.bytes = buf.makeIterator()
            return self.bytes.next()
        }
    }

    mutating func nextChar() throws -> Character? {
        if let ch = pushed {
            pushed = nil
            return ch
        }
        guard let b = try nextByte() else {
            return nil
        }
        return Character(Unicode.Scalar(b))
    }

    mutating func pushChar(_ ch: Character) throws {
        if let _ = pushed {
            throw RTFError.multiplePush
        }
        pushed = ch
    }
}

struct RTFParser {
    var lexer: RTFLexer

    init(path: String) throws {
        lexer = try RTFLexer(path: path)
    }

    // We vastly over-simplify the RTF format to make this more
    // simple.  All we are looking for is:
    // "{" "\\*" "\\cxs " strokes "}" definition "\r\n"
    // The definition can contain properly nested braces, but
    // otherwise, we just assemble all of this together as the
    // definition.  Anything else in the file is just ignored
    // entirely.
    mutating func next() throws -> Definition? {
        // How many prefixes have we seen: open, star, command-cxs.
        var state = States()
        while true {
            let tok = try lexer.next()
            if tok == .eof {
                return nil
            }
            try state.add(token: tok)
            if state.done {
                let strokes = state.strokes!
                let defn = state.definition
                return Definition(strokes: strokes, text: defn)
            }
        }
    }

    struct States {
        var state = 0
        var depth = 0
        var strokes: [Stroke]?
        var definition: [Token] = []

        mutating func add(token: Token) throws {
            if state == 0 && token == .open {
                state = 1
            } else if state == 1 && token == .star {
                state = 2
            } else if state == 2 && token == .command("cxs") {
                state = 3
            } else if state == 3 {
                if case .text(let stroke) = token {
                    if let _ = strokes {
                        throw RTFError.unexpectedToken(token: token)
                    }
                    strokes = try stroke.split(separator: "/")
                        .map { try Stroke(text: String($0)) }
                } else if token == .close {
                    state = 4
                } else {
                    throw RTFError.unexpectedToken(token: token)
                }
            } else if state == 4 {
                if depth == 0 && token == .newline {
                    state = 5
                } else {
                    definition.append(token)
                    switch token {
                    case .open:
                        depth += 1
                    case .close:
                        depth -= 1
                    default:
                        break
                    }
                }
            } else {
                state = 0
            }
        }

        var done: Bool {
            get {
                state == 5
            }
        }
    }
}

/*
func testParse(path: String) throws {
    var lexer = try RTFLexer(path: path)
    // for i in 1 ... 10000 {
    while true {
        let token = try lexer.next()
        // print("\(token)")
        if token == .eof {
            break
        }
    }
}
*/

func testParse(path: String) throws {
    var parser = try RTFParser(path: path)
    while let defn = try parser.next() {
        print("defn: \(defn)")
    }
}

try testParse(path: "../phoenix/phoenix.rtf")

/// Read a single stroke, defined as characters terminated by Space.
/// This can return nil if Escape was pressed, or throw an exception
/// trying to decode the final stroke.  This assumes it is called from
/// Raw mode.
func getStroke() throws -> Stroke? {
    var buf = ""
    while true {
        guard let ch = readCharacter() else {
            return nil
        }
        if ch == "\u{1b}" {
            return nil
        }
        if ch == " " {
            break
        }
        buf.append(ch)
    }
    return try Stroke(text: buf)
}

if false {
    try runInRawMode {
        cooked in
        print("This is typey\r")
        while true {
            do {
                guard let st = try getStroke() else {
                    break
                }
                try cooked {
                    print("stroke: \(st.ToCompact())")
                }
            } catch Stroke.StrokeError.invalidStroke(let reason, let text) {
                try cooked { print("Invalid stroke: \(reason) in \(text)") }
                continue
            }
        }
        let st = try Stroke(text: "SWR-Z")
        try cooked {
            print("st: \(st.bits)")
            print("st: \(st.ToCompact())")
            print("st: \(st)")
            print("st: \(String(st))")
        }
    }
}
