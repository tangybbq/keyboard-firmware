import AppKit
import XCTest
@testable import TaipoTeacherApp

/// The menu bar item is the app's only way in: no Dock icon, no window at launch.  A symbol
/// name SF Symbols does not have draws nothing, and the item vanishes with it.
final class MenuBarSymbolTests: XCTestCase {
    func testEverySymbolExists() {
        for symbol in DeviceMonitor.MenuBarSymbol.allCases {
            XCTAssertNotNil(
                NSImage(systemSymbolName: symbol.rawValue, accessibilityDescription: nil),
                "\(symbol) is \"\(symbol.rawValue)\", which SF Symbols does not have")
        }
    }
}
