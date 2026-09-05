import Foundation

@testable import TaipoKit

/// The header a real log starts with.
///
/// Every fixture needs one, because deriving anything from a session now depends on its
/// fingerprint agreeing with the tables in hand -- which is the point of the header, and
/// was the point of it before anything read it.
///
/// And a `variant` marker, so that a fixture says which table it was typed in rather than
/// leaning on whichever one the engine comes up in.  That default has changed once, and a
/// fixture that depends on it is a test that quietly starts measuring something else.
func sessionHeader(_ layouts: Layouts, variant: String = "taipo") -> String {
    "# session device=test boot_id=0x1 layout=\(layouts.fingerprint)\n"
        + "0 = variant \(variant == "dosh" ? 1 : 0)\n"
}
