import XCTest

@testable import TaipoKit

/// Reading logs written against an older set of tables.
final class LayoutHistoryTests: XCTestCase {
    private func layouts() throws -> Layouts { try Layouts.bundled() }

    /// The tables in hand are always in the history, or a change made now would strand
    /// every log written since the last one.
    ///
    /// Recording is a step in `scripts/sync-layouts.py`, and this is what says whether it
    /// was run.  A failure here means the tables changed and the revision they replaced
    /// was never written down -- fix it by checking the previous `layouts.json` out of git
    /// and running the script against it before going on.
    func testTheCurrentTablesAreRecorded() throws {
        let layouts = try layouts()
        let changed = LayoutHistory.bundled()
            .changedCodes(since: layouts.fingerprintValue, layouts: layouts)

        XCTAssertEqual(changed, [], "the tables in hand are not in layout-history.json")
    }

    /// A layout nothing has recorded cannot be interpreted, and says so.
    func testAnUnknownLayoutIsNotGuessedAt() throws {
        let layouts = try layouts()
        let history = LayoutHistory.bundled()

        XCTAssertNil(history.changedCodes(since: 0xdead_beef_dead_beef, layouts: layouts))
        XCTAssertNil(history.changedCodes(since: nil, layouts: layouts), "no fingerprint")
    }

    /// A revision that differs in one chord costs that chord and nothing else.
    ///
    /// This is the whole point: moving the apostrophe should reset the apostrophe, because
    /// it is a new movement and has to be learned again, and should leave the rest of the
    /// ladder alone.
    func testOnlyTheChordsThatMovedAreLost() throws {
        let layouts = try layouts()
        let dosh = try XCTUnwrap(layouts.variants["dosh"])

        // The tables as they are, with one chord given a different meaning and one taken
        // away entirely.
        let moved = try XCTUnwrap(dosh.chords.first { $0.action.types == "," })
        let removed = try XCTUnwrap(dosh.chords.first { $0.action.types == "z" })
        var signatures = [String: String]()
        for chord in dosh.chords where chord.code != removed.code {
            signatures[String(chord.code)] =
                chord.code == moved.code
                ? "key:Grave" : LayoutHistory.signature(chord.action)
        }
        var taipo = [String: String]()
        for chord in try XCTUnwrap(layouts.variants["taipo"]).chords {
            taipo[String(chord.code)] = LayoutHistory.signature(chord.action)
        }

        let history = LayoutHistory(revisions: [
            LayoutHistory.Revision(
                fingerprint: "0x0000000000000001",
                variants: ["dosh": signatures, "taipo": taipo])
        ])

        let changed = try XCTUnwrap(history.changedCodes(since: 1, layouts: layouts))
        XCTAssertEqual(changed, [moved.code, removed.code])
    }

    /// A chord in one variant changing does not cost the other variant anything.
    func testTheVariantsAreDiffedApart() throws {
        let layouts = try layouts()
        var variants = [String: [String: String]]()
        for (name, variant) in layouts.variants {
            variants[name] = Dictionary(
                uniqueKeysWithValues: variant.chords.map {
                    (String($0.code), LayoutHistory.signature($0.action))
                })
        }
        // One taipo chord means something else in the old revision.
        let taipoChord = try XCTUnwrap(layouts.variants["taipo"]?.chords.first)
        variants["taipo"]?[String(taipoChord.code)] = "key:Grave"

        let history = LayoutHistory(revisions: [
            LayoutHistory.Revision(fingerprint: "0x0000000000000002", variants: variants)
        ])

        XCTAssertEqual(
            history.changedCodes(since: 2, layouts: layouts), [taipoChord.code])
    }

    /// Signatures tell apart the things a chord can be, so a change of kind is a change.
    func testSignaturesDistinguishActions() throws {
        let layouts = try layouts()
        let dosh = try XCTUnwrap(layouts.variants["dosh"])
        let a = try XCTUnwrap(dosh.chords.first { $0.action.types == "a" })
        let capitalA = try XCTUnwrap(dosh.chords.first { $0.action.types == "A" })
        let shift = try XCTUnwrap(dosh.chords.first { $0.action.kind == "oneshot" })

        XCTAssertNotEqual(
            LayoutHistory.signature(a.action), LayoutHistory.signature(capitalA.action),
            "a letter and its capital are different chords to learn")
        XCTAssertTrue(LayoutHistory.signature(shift.action).hasPrefix("oneshot:"))
    }
}
