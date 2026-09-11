import XCTest

@testable import TaipoKit

/// The Swift port of the composition rules has to agree with the Rust crate.
///
/// `golden/orsy-theory.sample` is every 37th line of `cargo run --example dump` in
/// `bbq-orsy`, which lists every translatable chord with its text and spacing flags.  The
/// Rust side is checked against the Python model over the whole chord space by
/// `midi4text-analysis/check_rust.py`; this sample is what keeps a third implementation
/// from drifting.  Regenerate it with
///
///     cd bbq-orsy && cargo run --release --example dump | awk 'NR % 37 == 1' \
///         > ../taipo-teacher/Tests/TaipoKitTests/golden/orsy-theory.sample
final class OrsyTheoryTests: XCTestCase {
    private func theory() throws -> OrsyTheory {
        OrsyTheory(try XCTUnwrap((try Layouts.bundled()).orsy, "layouts.json has no orsy"))
    }

    /// Every sampled chord translates to the same text and spacing.
    func testSampleMatchesRust() throws {
        let t = try theory()
        let url = try XCTUnwrap(
            Bundle.module.url(forResource: "golden/orsy-theory", withExtension: "sample"))
        let text = try String(contentsOf: url, encoding: .utf8)
        var checked = 0
        for line in text.split(separator: "\n") {
            let fields = line.split(separator: " ", maxSplits: 4, omittingEmptySubsequences: false)
            guard fields.count == 5, let left = UInt16(fields[0], radix: 16),
                let right = UInt16(fields[1], radix: 16)
            else { XCTFail("bad sample line: \(line)"); continue }
            let want = (String(fields[4]), fields[2] == "1", fields[3] == "1")
            guard let got = t.translate(left: left, right: right) else {
                XCTFail(String(format: "0x%03x-0x%03x untranslatable, Rust says %@", left, right, want.0))
                continue
            }
            XCTAssertEqual(got.text, want.0, String(format: "0x%03x-0x%03x", left, right))
            XCTAssertEqual(got.spaceBefore, want.1, String(format: "0x%03x-0x%03x before", left, right))
            XCTAssertEqual(got.spaceAfter, want.2, String(format: "0x%03x-0x%03x after", left, right))
            checked += 1
        }
        XCTAssertGreaterThan(checked, 4000)
    }

    /// The commands, the upper pinky and a stray combination are not syllables.
    func testOutcomes() throws {
        let t = try theory()
        XCTAssertEqual(t.outcome(left: 0x388, right: 0), .doshToggle)
        XCTAssertEqual(t.outcome(left: 0x380, right: 0x100), .dosh(0x100))
        XCTAssertEqual(t.outcome(left: 0x067, right: 0), .undo)
        XCTAssertEqual(t.outcome(left: 0, right: 0x200), .space)
        XCTAssertEqual(t.outcome(left: 0, right: 0x188), .capNext)
        XCTAssertEqual(t.outcome(left: 0x380, right: 0), .dead)
        XCTAssertEqual(t.outcome(left: 0x014, right: 0x008), .dead)
        // `ten`, closing the word: t, e+Bk, n.
        guard case .text(let ten) = t.outcome(left: 0x004, right: 0x248) else {
            return XCTFail("ten is a syllable")
        }
        XCTAssertEqual(ten.text, "ten")
        XCTAssertEqual(ten.patterns, OrsyPatterns(onset: "FP", second: "", vowel: "ue", coda: "N"))
        XCTAssertEqual(ten.patterns.keys, ["s1:FP", "s3:ue", "s4:N"])
        XCTAssertEqual(ten.rules, 0)
    }

    /// The rules are reported, and the flags agree with the names the export carries.
    func testRules() throws {
        let t = try theory()
        for rule in t.tables.rules {
            XCTAssertEqual(OrsyTheory.Rules.byName[rule.name], rule.flag, rule.name)
        }
        // time: t + mirrored i + m.
        XCTAssertEqual(t.translate(left: 0x084, right: 0x043)?.rules, OrsyTheory.Rules.mirrored)
        XCTAssertEqual(t.translate(left: 0x084, right: 0x043)?.text, "time")
        // quit: c + c(XIU) is qu.
        XCTAssertEqual(t.translate(left: 0x18a, right: 0x084)?.rules, OrsyTheory.Rules.cluster)
        XCTAssertEqual(t.translate(left: 0x18a, right: 0x084)?.text, "quit")
    }
}
