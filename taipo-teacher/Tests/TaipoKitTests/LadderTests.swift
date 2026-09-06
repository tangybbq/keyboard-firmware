import XCTest

@testable import TaipoKit

/// The ladder, and the material it asks for.
final class LadderTests: XCTestCase {
    private func layouts() throws -> Layouts { try Layouts.bundled() }

    /// A skill model in which the first `count` items of the ladder are learned cold and
    /// nothing else has ever been typed.
    private func model(learning count: Int, of items: [LadderItem]) -> SkillModel {
        var skills = [UInt16: ChordSkill]()
        for item in items.prefix(count) {
            for code in item.codes {
                skills[code] = ChordSkill(code: code, count: 50, medianMs: 400, deleted: 0)
            }
        }
        return SkillModel(skills: skills, sessions: 1, chords: 50 * count)
    }

    private func ladder(learning count: Int, variant: String = "dosh") throws -> Ladder {
        let layouts = try layouts()
        let items = Ladder.order(
            layouts: layouts, variant: variant, options: Ladder.Options())
        return Ladder(
            layouts: layouts, variant: variant, skill: model(learning: count, of: items))
    }

    // MARK: - The order

    /// Letters first, by frequency, with the marks and digits woven in later.
    func testOrderTeachesLettersFirst() throws {
        let items = Ladder.order(
            layouts: try layouts(), variant: "dosh", options: Ladder.Options())

        XCTAssertEqual(items.prefix(7).map(\.label).joined(), "etaoins")
        XCTAssertTrue(
            items.prefix(10).allSatisfy { $0.stage == .letter },
            "nothing but letters before interleaveAfter")

        // Every letter is on the ladder, and they are all done before the tail of symbols.
        let letters = items.filter { $0.stage == .letter }.map(\.label).sorted().joined()
        XCTAssertEqual(letters, "abcdefghijklmnopqrstuvwxyz")

        // Prose wants a period and a comma long before it wants a digit.
        let period = try XCTUnwrap(items.firstIndex { $0.label == "." })
        let comma = try XCTUnwrap(items.firstIndex { $0.label == "," })
        let firstDigit = try XCTUnwrap(items.firstIndex { $0.stage == .digit })
        XCTAssertLessThan(period, comma)
        XCTAssertLessThan(comma, firstDigit)
    }

    /// Brackets are taught as a pair, because half a bracket has nothing to practise on.
    func testPairsAreOneItem() throws {
        let items = Ladder.order(
            layouts: try layouts(), variant: "dosh", options: Ladder.Options())
        let parens = try XCTUnwrap(items.first { $0.label == "()" })

        XCTAssertEqual(parens.codes.count, 2)
        XCTAssertEqual(parens.stage, .punctuation)
        // And neither half is also on the ladder by itself.
        XCTAssertNil(items.first { $0.label == "(" })
        XCTAssertNil(items.first { $0.label == ")" })
    }

    /// The ladder is about characters, not chords, so both tables teach the same syllabus
    /// in the same order -- on different chords.
    func testBothVariantsTeachTheSameSyllabus() throws {
        let options = Ladder.Options()
        let taipo = Ladder.order(layouts: try layouts(), variant: "taipo", options: options)
        let dosh = Ladder.order(layouts: try layouts(), variant: "dosh", options: options)

        XCTAssertEqual(taipo.map(\.label), dosh.map(\.label))
        XCTAssertNotEqual(taipo.map(\.codes), dosh.map(\.codes))
    }

    // MARK: - Unlocking

    /// Nothing typed yet: the opening set, and every one of them in need of work.
    func testColdStart() throws {
        let ladder = try ladder(learning: 0)

        XCTAssertEqual(ladder.unlockedCount, 7)
        XCTAssertEqual(ladder.unlocked.map(\.label).joined(), "etaoins")
        XCTAssertEqual(ladder.focus.count, 3)
        XCTAssertEqual(ladder.focus.map(\.label), ["e", "t", "a"])
    }

    /// Learning what is out unlocks more, and the focus follows the frontier.
    ///
    /// Two at a time, not three: the focus set holds three, and one place is kept for an
    /// item that is past the gate but still slow.  See `testEveryUnreachedItemIsDrilled`
    /// for why every item short of the gate has to be in the set.
    func testLearningUnlocksMore() throws {
        let seven = try ladder(learning: 7)
        XCTAssertEqual(seven.unlockedCount, 9)
        XCTAssertEqual(seven.focus.map(\.label), ["r", "h"])

        // With the first twenty learned, two more come out and those two -- the only ones
        // short of the gate -- are exactly what the focus set points at.  Nothing else is
        // offered a place, because everything else is already fluent.
        let twenty = try ladder(learning: 20)
        XCTAssertEqual(twenty.unlockedCount, 22)
        XCTAssertEqual(
            Set(twenty.focus.map(\.label)),
            Set(twenty.items[20..<22].map(\.label)))
    }

    /// Every item short of the gate is drilled.
    ///
    /// The one that is not is the one that stalls the ladder: an item unlocked and then
    /// left out of the focus set gets no deliberate practice, so it never reaches the
    /// gate, so nothing behind it ever unlocks.  That happened -- `b` came out and sat
    /// there while the set showed the same three names, one of which was a slow comma
    /// holding the polish place.
    func testEveryUnreachedItemIsDrilled() throws {
        let layouts = try layouts()
        let items = Ladder.order(
            layouts: layouts, variant: "dosh", options: Ladder.Options())

        // Reached but slow, so it wants the polish place -- and must not take one that an
        // item short of the gate needs.
        var skills = [UInt16: ChordSkill]()
        for item in items.prefix(14) {
            for code in item.codes {
                skills[code] = ChordSkill(code: code, count: 200, medianMs: 300, deleted: 0)
            }
        }
        for code in items[14].codes {
            skills[code] = ChordSkill(code: code, count: 125, medianMs: 1021, deleted: 12)
        }
        let skill = SkillModel(skills: skills, sessions: 1, chords: 3000)
        let ladder = Ladder(layouts: layouts, variant: "dosh", skill: skill)

        let unreached = ladder.unlocked.filter { !$0.codes.allSatisfy(skill.reached) }
        XCTAssertFalse(unreached.isEmpty)
        for item in unreached {
            XCTAssertTrue(
                ladder.focus.contains(item),
                "\(item.label) is unlocked and short of the gate but never drilled")
        }
        // And the slow one still gets the place that is left.
        XCTAssertTrue(ladder.focus.contains { $0.label == items[14].label })
    }

    /// The whole point of the change from keybr: one stubborn item does not stop the
    /// ladder, it just stays in the focus set.
    func testAStubbornItemDoesNotBlock() throws {
        let layouts = try layouts()
        let items = Ladder.order(
            layouts: layouts, variant: "dosh", options: Ladder.Options())

        // Everything up to item 20 learned except the very first one, which is hopeless.
        var skills = [UInt16: ChordSkill]()
        for item in items.prefix(20).dropFirst() {
            for code in item.codes {
                skills[code] = ChordSkill(code: code, count: 50, medianMs: 400, deleted: 0)
            }
        }
        for code in items[0].codes {
            skills[code] = ChordSkill(code: code, count: 200, medianMs: 3000, deleted: 40)
        }
        let ladder = Ladder(
            layouts: layouts, variant: "dosh",
            skill: SkillModel(skills: skills, sessions: 1, chords: 1000))

        // One more unlocked on top of the twenty that are learned, and the stubborn one is
        // still being worked rather than left behind.
        XCTAssertEqual(ladder.unlockedCount, 21)
        XCTAssertTrue(ladder.focus.contains { $0.label == items[0].label })
    }

    /// Speed does not gate the ladder, so a chord you can type but are slow at lets the
    /// next one in -- and keeps a place in the focus set while it is still slow.
    ///
    /// This is the developer's case: the comma measured 120 uses, 4% taken back and just
    /// over a second, which is reached but far from fluent.  Under the old rule it stopped
    /// the ladder dead, and the marks behind it -- the ones whose absence sends a writer
    /// back to Taipo -- never arrived.
    func testSlowButReachedStillAdvancesTheLadder() throws {
        let layouts = try layouts()
        let items = Ladder.order(
            layouts: layouts, variant: "dosh", options: Ladder.Options())

        var skills = [UInt16: ChordSkill]()
        for item in items.prefix(12) {
            for code in item.codes {
                skills[code] = ChordSkill(code: code, count: 50, medianMs: 300, deleted: 0)
            }
        }
        // Item 12: typed plenty, hardly ever taken back, and slow.
        for code in items[12].codes {
            skills[code] = ChordSkill(code: code, count: 120, medianMs: 1050, deleted: 5)
        }
        let skill = SkillModel(skills: skills, sessions: 1, chords: 700)
        let ladder = Ladder(layouts: layouts, variant: "dosh", skill: skill)

        XCTAssertTrue(items[12].codes.allSatisfy(skill.reached))
        XCTAssertFalse(items[12].codes.allSatisfy(skill.learned), "reached but not fluent")

        // Thirteen reached, so two more come out on top of them.
        XCTAssertEqual(ladder.unlockedCount, 15)
        // And the slow one holds a place, rather than being crowded out by the new ones.
        XCTAssertTrue(
            ladder.focus.contains { $0.label == items[12].label },
            "the slow item stopped being practised")
        XCTAssertEqual(ladder.focus.count, 3)
    }

    /// Practice cannot take the ladder backwards.
    ///
    /// The median is over recent uses, so drilling a chord you are slow at pushes it down.
    /// While speed gated the ladder that could un-learn an item and cost the writer items
    /// they already had -- which is what happened, 19 back to 15, in the middle of a
    /// session.  Uses only grow, so the gate cannot move that way now.
    func testGettingSlowerDoesNotTakeItemsAway() throws {
        let layouts = try layouts()
        let items = Ladder.order(
            layouts: layouts, variant: "dosh", options: Ladder.Options())

        func ladder(medianMs: UInt32) -> Ladder {
            var skills = [UInt16: ChordSkill]()
            for item in items.prefix(15) {
                for code in item.codes {
                    skills[code] = ChordSkill(
                        code: code, count: 200, medianMs: medianMs, deleted: 2)
                }
            }
            return Ladder(
                layouts: layouts, variant: "dosh",
                skill: SkillModel(skills: skills, sessions: 1, chords: 3000))
        }

        let brisk = ladder(medianMs: 300)
        let bogged = ladder(medianMs: 1400)
        XCTAssertEqual(brisk.unlockedCount, 17)
        XCTAssertEqual(bogged.unlockedCount, brisk.unlockedCount, "the ladder went backwards")
    }

    /// A finished ladder still has something to practise.
    func testACompleteLadderStillFocuses() throws {
        let layouts = try layouts()
        let items = Ladder.order(
            layouts: layouts, variant: "dosh", options: Ladder.Options())
        let ladder = try self.ladder(learning: items.count)

        XCTAssertTrue(ladder.complete)
        XCTAssertEqual(ladder.focus.count, 3)
    }

    // MARK: - Choosing what to work on

    /// Everything reached, letters fast apart from `slow`, every mark and digit slow.
    ///
    /// The finished ladder as it actually measures: a mark is typed where the writer is
    /// deciding what comes next, so the thinking pause is charged to it and its median
    /// stays well over a letter's however well it is known.
    private func matured(_ items: [LadderItem], slowLetters slow: Set<String>) -> SkillModel {
        var skills = [UInt16: ChordSkill]()
        for item in items {
            let ms: UInt32 =
                item.stage == .letter ? (slow.contains(item.label) ? 900 : 350) : 1400
            for code in item.codes {
                skills[code] = ChordSkill(code: code, count: 200, medianMs: ms, deleted: 0)
            }
        }
        return SkillModel(skills: skills, sessions: 1, chords: 200 * items.count)
    }

    /// Left alone, a finished ladder drills nothing but marks and digits.
    ///
    /// Not a bug in the ranking so much as the reason the switches exist: confidence is
    /// the only thing left to rank by once everything is reached, and punctuation loses it
    /// permanently.
    func testAFinishedLadderFillsUpWithMarks() throws {
        let layouts = try layouts()
        let items = Ladder.order(
            layouts: layouts, variant: "dosh", options: Ladder.Options())
        let ladder = Ladder(
            layouts: layouts, variant: "dosh",
            skill: matured(items, slowLetters: ["q", "z", "x"]))

        XCTAssertTrue(ladder.complete)
        XCTAssertTrue(ladder.focus.allSatisfy { $0.stage != .letter })
    }

    /// Switching the other stages off puts the weakest letters back in front.
    func testStagesNarrowTheFocusSet() throws {
        let layouts = try layouts()
        let items = Ladder.order(
            layouts: layouts, variant: "dosh", options: Ladder.Options())
        let ladder = Ladder(
            layouts: layouts, variant: "dosh",
            skill: matured(items, slowLetters: ["q", "z", "x"]),
            options: Ladder.Options(stages: [.letter]))

        XCTAssertEqual(Set(ladder.focus.map(\.label)), ["q", "z", "x"])
        // And the ladder itself is untouched: the switch says what to drill, not what has
        // been learned.
        XCTAssertTrue(ladder.complete)
        XCTAssertTrue(ladder.deferred.isEmpty)
    }

    /// Letters only means letters only: no decorations, not even for circulation.
    func testStagesKeepMarksOutOfTheMaterial() throws {
        let layouts = try layouts()
        let items = Ladder.order(
            layouts: layouts, variant: "dosh", options: Ladder.Options())
        let skill = matured(items, slowLetters: ["q", "z", "x"])
        let maker = LadderMaker(layouts: layouts, variant: "dosh")

        // With everything switched on the lines carry marks, which is what the complaint
        // was about; with only letters they carry none.
        let all = maker.drill(
            Ladder(layouts: layouts, variant: "dosh", skill: skill), lines: 8, seed: 99)
        XCTAssertTrue(all.lines.contains { $0.contains { !$0.isLetter && $0 != " " } })

        let letters = maker.drill(
            Ladder(
                layouts: layouts, variant: "dosh", skill: skill,
                options: Ladder.Options(stages: [.letter])), lines: 8, seed: 99)
        XCTAssertFalse(letters.lines.isEmpty)
        for line in letters.lines {
            XCTAssertTrue(
                line.allSatisfy { $0.isLowercase || $0 == " " },
                "a letters-only drill asked for \(line)")
        }
    }

    /// A switch turned off in front of something still being learned stalls the ladder,
    /// and says which item it is stalled on.
    func testDeferredNamesWhatIsHeldBack() throws {
        let layouts = try layouts()
        let items = Ladder.order(
            layouts: layouts, variant: "dosh", options: Ladder.Options())

        // The first point at which the ladder is waiting on something that is not a letter.
        let learned = try XCTUnwrap(
            (1..<items.count).first { n in
                Ladder(layouts: layouts, variant: "dosh", skill: model(learning: n, of: items))
                    .unlocked.dropFirst(n).contains { $0.stage != .letter }
            })
        let skill = model(learning: learned, of: items)
        let open = Ladder(layouts: layouts, variant: "dosh", skill: skill)
        let narrowed = Ladder(
            layouts: layouts, variant: "dosh", skill: skill,
            options: Ladder.Options(stages: [.letter]))

        let held = open.unlocked.dropFirst(learned).filter { $0.stage != .letter }
        XCTAssertEqual(narrowed.deferred.map(\.label), held.map(\.label))
        // Held back means not drilled, and unlocking is unchanged either way -- which is
        // exactly why the ladder cannot move past them until the switch goes back on.
        for item in held {
            XCTAssertFalse(narrowed.focus.contains(item))
        }
        XCTAssertEqual(narrowed.unlockedCount, open.unlockedCount)
    }

    /// Every switch off is not a reason to stop drilling.
    ///
    /// The screen will not allow it, but a defaults file can hold it and a drill still has
    /// to ask for something.
    func testNoStagesAtAllStillDrills() throws {
        let layouts = try layouts()
        let items = Ladder.order(
            layouts: layouts, variant: "dosh", options: Ladder.Options())
        let ladder = Ladder(
            layouts: layouts, variant: "dosh",
            skill: matured(items, slowLetters: ["q"]),
            options: Ladder.Options(stages: []))

        XCTAssertEqual(ladder.focus.count, 3)
    }

    // MARK: - The material

    /// The rule the material must never break: nothing in a line needs a chord the ladder
    /// has not introduced.
    func testLinesUseOnlyUnlockedCharacters() throws {
        let maker = LadderMaker(layouts: try layouts(), variant: "dosh")
        for learned in [0, 9, 14, 18, 25, 33, 45, 60] {
            let ladder = try ladder(learning: learned)
            var allowed = Set<Character>(" ")
            for item in ladder.unlocked { allowed.formUnion(item.label) }

            for line in maker.drill(ladder, lines: 12, seed: 4321).lines {
                for ch in line {
                    XCTAssertTrue(
                        allowed.contains(ch), "\(ch) is not unlocked at \(learned): \(line)")
                }
            }
        }
    }

    /// And the rule that makes it a drill: the chords it asks for are chords the table
    /// actually has, so every line segments completely.
    func testEveryLineIsTypeable() throws {
        let layouts = try layouts()
        let maker = LadderMaker(layouts: layouts, variant: "dosh")
        for learned in [0, 14, 33, 60] {
            let ladder = try ladder(learning: learned)
            for line in maker.drill(ladder, lines: 8, seed: 77).lines {
                let target = DrillTarget(text: line, layouts: layouts, variant: "dosh")
                XCTAssertEqual(
                    target.units.map(\.text.count).reduce(0, +), line.count,
                    "dosh cannot type all of: \(line)")
            }
        }
    }

    /// Every focus item is worked on every line, which is what stops the drill from
    /// wandering off the thing it is meant to be teaching.
    func testEveryFocusItemAppearsInEveryLine() throws {
        let maker = LadderMaker(layouts: try layouts(), variant: "dosh")
        for learned in [0, 9, 14, 18, 25, 33] {
            let ladder = try ladder(learning: learned)
            for line in maker.drill(ladder, lines: 6, seed: 555).lines {
                for item in ladder.focus {
                    let wanted = item.stage == .digit ? item.label : String(item.label.first!)
                    XCTAssertTrue(
                        line.contains(wanted),
                        "\(item.label) missing from \"\(line)\" at \(learned)")
                }
            }
        }
    }

    /// But it never takes the line over: most of a line is ordinary words.
    ///
    /// This is the "less insistent than keybr" property, and it is worth pinning: the
    /// failure it guards against is a line of `,,, ,,, ,,,` that technically drills the
    /// comma.  Two things say it is still typing: most of the characters are letters, and
    /// some whole words are left untouched, which is the rule the material actually
    /// enforces when it decides how many words it may decorate.
    func testFocusDoesNotTakeOverTheLine() throws {
        let maker = LadderMaker(layouts: try layouts(), variant: "dosh")
        for learned in [14, 18, 25, 45] {
            let ladder = try ladder(learning: learned)
            for line in maker.drill(ladder, lines: 6, seed: 909).lines {
                let letters = line.filter { $0.isLetter }.count
                XCTAssertGreaterThan(
                    letters, line.count / 2, "\(line) is more punctuation than typing")
                let plain = line.split(separator: " ").filter { $0.allSatisfy(\.isLetter) }
                XCTAssertGreaterThanOrEqual(
                    plain.count, 2, "nothing left plain in \(line)")
            }
        }
    }

    /// A mark or a digit gets as much practice in a line as a letter does.
    ///
    /// It did not, and that is what this pins.  A focus letter is worked by every word
    /// that happens to contain it, which runs to two or three uses a line; a mark is
    /// worked only where one is deliberately put, and there was exactly one of those, so
    /// marks took two to three times as long to learn as the letters around them.
    func testMarksGetAsMuchPracticeAsLetters() throws {
        let maker = LadderMaker(layouts: try layouts(), variant: "dosh")
        // Ladder states whose focus sets hold a mark or a digit: the comma, the zero, the
        // apostrophe, and one deep enough that all three are marks at once.
        for learned in [14, 18, 22, 34] {
            let ladder = try ladder(learning: learned)
            let extras = ladder.focus.filter { $0.stage != .letter }
            XCTAssertFalse(extras.isEmpty, "no mark in focus at \(learned)")

            let lines = maker.drill(ladder, lines: 40, seed: 31).lines
            for item in extras {
                let ch = try XCTUnwrap(item.label.first)
                let total = lines.reduce(0) { $0 + $1.filter { c in c == ch }.count }
                let perLine = Double(total) / Double(lines.count)
                XCTAssertGreaterThanOrEqual(
                    perLine, 2, "\(item.label) only \(perLine) a line at \(learned)")
            }
        }
    }

    /// A line stays short enough to see the end of what you have started.
    ///
    /// Not a hard limit: the cap gives way rather than dropping practice, so a crowded
    /// line can run a little past it.  What it may not do is run to the 174 characters it
    /// reached when marks first started being placed as often as letters -- five wrapped
    /// lines of the target, and enough to be truncated on screen.
    func testLinesStayShortEnoughToRead() throws {
        let maker = LadderMaker(layouts: try layouts(), variant: "dosh")
        var longest = 0
        var over = 0
        var lines = 0
        for learned in stride(from: 0, through: 56, by: 4) {
            let ladder = try ladder(learning: learned)
            for line in maker.drill(ladder, lines: 25, seed: 606).lines {
                longest = max(longest, line.count)
                if line.count > LadderMaker.maxCharacters { over += 1 }
                lines += 1
            }
        }
        // A little over is allowed; a lot, or often, is not.
        XCTAssertLessThanOrEqual(longest, LadderMaker.maxCharacters * 5 / 4, "longest line")
        XCTAssertLessThan(over * 10, lines, "more than a tenth of lines are over the cap")
    }

    /// Nothing already learned drops out of circulation entirely.
    ///
    /// Reinforcement is biased toward what was learned most recently, and an earlier cut
    /// at that used a hard window, which dropped the period and the comma -- the two marks
    /// most worth keeping a hand in -- to exactly never once four more had been learned.
    func testLearnedMarksStayInCirculation() throws {
        let maker = LadderMaker(layouts: try layouts(), variant: "dosh")
        let ladder = try ladder(learning: 45)
        let older = ladder.unlocked.filter { $0.stage != .letter && !ladder.focus.contains($0) }
        XCTAssertGreaterThan(older.count, 6, "not enough learned marks to be a test")

        let lines = maker.drill(ladder, lines: 400, seed: 5).lines
        for item in older {
            let ch = try XCTUnwrap(item.label.first)
            XCTAssertTrue(
                lines.contains { $0.contains(ch) }, "\(item.label) never appears at all")
        }
    }

    /// The same ladder and seed give the same lines, so a test can say anything at all
    /// about them.
    func testMaterialIsDeterministic() throws {
        let maker = LadderMaker(layouts: try layouts(), variant: "dosh")
        let ladder = try ladder(learning: 18)

        XCTAssertEqual(
            maker.drill(ladder, seed: 12345).lines, maker.drill(ladder, seed: 12345).lines)
        XCTAssertNotEqual(
            maker.drill(ladder, seed: 12345).lines, maker.drill(ladder, seed: 999).lines)
    }

    /// Numbers use the digits that are out, and no others.
    func testNumbersUseOnlyUnlockedDigits() throws {
        let maker = LadderMaker(layouts: try layouts(), variant: "dosh")
        var rng = DrillRandom(seed: 8)

        // One digit out: a one-digit number, never `000`.
        for _ in 0..<20 {
            XCTAssertEqual(maker.number("0", digits: ["0"], using: &rng), "0")
        }

        // Three out: at most three digits, always including the one being taught, and
        // never with a leading zero.
        for _ in 0..<50 {
            let n = maker.number("2", digits: ["0", "1", "2"], using: &rng)
            XCTAssertTrue(n.contains("2"), n)
            XCTAssertTrue(n.allSatisfy { "012".contains($0) }, n)
            XCTAssertLessThanOrEqual(n.count, 3, n)
            if n.count > 1 { XCTAssertNotEqual(n.first, "0", n) }
        }
    }

    /// The heading says where the ladder is and what it is working on.
    func testTitle() throws {
        XCTAssertEqual(try ladder(learning: 0).title(), "Ladder — 7 of 64; on \"e\" \"t\" \"a\"")
    }
}
