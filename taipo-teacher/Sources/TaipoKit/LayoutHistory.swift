import Foundation

/// What the chord tables used to be, so that logs written against them can still be read.
///
/// A session records the layout fingerprint the device reported, and anything derived from
/// it has to agree with the tables in hand or the replay is quietly wrong -- the chord that
/// typed `a` before Dosh's letters moved types `s` now.  With only a fingerprint to go on
/// the answer can only be all or nothing, and it was nothing: change one chord and every
/// session ever logged is skipped.
///
/// That is too blunt for a layout still being worked on.  Moving the apostrophe should cost
/// the apostrophe's learning, because it is a new movement and has to be learned again --
/// and it should cost nothing else.
///
/// So each revision of the tables is written down as it goes by, in
/// `layout-history.json`, as the action of every chord in both variants.  Comparing an old
/// revision against the tables in hand gives exactly the chords whose meaning changed, and
/// a session logged under it can be replayed for everything else.
///
/// A revision that was never recorded cannot be interpreted, and its sessions are skipped
/// whole, as they were before.  Recording is `taipo-teacher/scripts/sync-layouts.py`, which
/// is also what copies the tables into this package -- one step, so that the two cannot
/// come apart.
public struct LayoutHistory {
    struct Revision: Decodable {
        let fingerprint: String
        /// variant -> chord code, in decimal -> what the chord did.
        let variants: [String: [String: String]]
    }

    private let revisions: [Revision]

    init(revisions: [Revision]) { self.revisions = revisions }

    /// The history shipped in this module.
    public static func bundled() -> LayoutHistory {
        struct File: Decodable { let revisions: [Revision] }
        guard let url = Bundle.module.url(forResource: "layout-history", withExtension: "json"),
            let data = try? Data(contentsOf: url),
            let file = try? JSONDecoder().decode(File.self, from: data)
        else { return LayoutHistory(revisions: []) }
        return LayoutHistory(revisions: file.revisions)
    }

    /// What a chord does, written the same way the recorder writes it.
    static func signature(_ action: Layouts.Action) -> String {
        let detail =
            action.key ?? action.text ?? action.mods?.joined(separator: "+").nilIfEmpty
        guard let detail, !detail.isEmpty else { return action.kind }
        return "\(action.kind):\(detail)"
    }

    /// The chords whose meaning has changed since the layout a session was logged under,
    /// or nil when that layout is not one this knows about.
    ///
    /// Nil is the honest answer for an unknown revision and means "do not derive from
    /// this": there is no way to tell which of its chords still mean what they did.
    public func changedCodes(since fingerprint: UInt64?, layouts: Layouts) -> Set<UInt16>? {
        guard let fingerprint else { return nil }
        if fingerprint == layouts.fingerprintValue { return [] }
        guard
            let old = revisions.first(where: {
                UInt64($0.fingerprint.dropFirst(2), radix: 16) == fingerprint
            })
        else { return nil }

        var changed = Set<UInt16>()
        for (name, variant) in layouts.variants {
            let before = old.variants[name] ?? [:]
            var seen = Set<UInt16>()
            for chord in variant.chords {
                seen.insert(chord.code)
                if before[String(chord.code)] != Self.signature(chord.action) {
                    changed.insert(chord.code)
                }
            }
            // A chord the old tables had and these do not has changed as surely as one
            // that moved: it used to type something and now types nothing.
            for (code, _) in before {
                if let code = UInt16(code), !seen.contains(code) { changed.insert(code) }
            }
        }
        return changed
    }
}

extension String {
    fileprivate var nilIfEmpty: String? { isEmpty ? nil : self }
}
