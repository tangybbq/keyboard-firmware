import Foundation

@testable import TaipoKit

/// The header a real log starts with.
///
/// Every fixture needs one, because deriving anything from a session now depends on its
/// fingerprint agreeing with the tables in hand -- which is the point of the header, and
/// was the point of it before anything read it.
func sessionHeader(_ layouts: Layouts) -> String {
    "# session device=test boot_id=0x1 layout=\(layouts.fingerprint)\n"
}
