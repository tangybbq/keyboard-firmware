import Foundation

/// A connection to a keyboard's minder service, over whichever link carries it.
///
/// Synchronous, as `MinderDevice` is: every call blocks its thread until the keyboard has
/// answered or the timeout has passed, so a link is used from one thread of the caller's
/// own, never the main one.
public protocol MinderLink: AnyObject {
    /// Send a request.
    func send(_ request: Request) throws

    /// Read one reply.
    func receive(timeout: TimeInterval) throws -> Reply

    /// Throw away anything the keyboard still owes, so a session starts in step.
    func drain()
}

extension MinderLink {
    /// Send and wait for the reply.
    public func call(_ request: Request, timeout: TimeInterval = 20.0) throws -> Reply {
        try send(request)
        return try receive(timeout: timeout)
    }
}
