import XCTest
@testable import TaipoTeacherApp

/// What the keyboard menu offers.
///
/// The chosen keyboard has to stay in the menu when it is unplugged: the app waits for it
/// rather than collecting from another, so a choice that vanished from the menu would be a
/// silent stop with no visible way to undo it.
final class KeyboardChoiceTests: XCTestCase {
    func testConnectedKeyboardsInOrder() {
        XCTAssertEqual(
            DeviceMonitor.keyboardChoices(connected: ["mesa3b-a", "mesa3b-b"], chosen: nil),
            ["mesa3b-a", "mesa3b-b"])
    }

    func testChosenAndPluggedInIsNotRepeated() {
        XCTAssertEqual(
            DeviceMonitor.keyboardChoices(connected: ["mesa3b-a", "mesa3b-b"], chosen: "mesa3b-b"),
            ["mesa3b-a", "mesa3b-b"])
    }

    func testChosenButUnpluggedStaysOffered() {
        XCTAssertEqual(
            DeviceMonitor.keyboardChoices(connected: ["mesa3b-a"], chosen: "mesa3b-b"),
            ["mesa3b-a", "mesa3b-b"])
        XCTAssertEqual(
            DeviceMonitor.keyboardChoices(connected: [], chosen: "mesa3b-b"), ["mesa3b-b"])
    }
}
