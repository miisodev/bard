//! One worker thread for Windows Credential Manager.
//!
//! Credential Manager is process-global, so every read, write and delete has to
//! be serialised against every other one. That was previously done with a mutex
//! per module — `license` and `cloud` each had their own, both guarding the same
//! store, so the serialisation each comment claimed did not exist.
//!
//! The mutex was also acquired *inside* a freshly spawned thread, one per call,
//! each abandoned on timeout while still holding it. A stalled vault therefore
//! leaked a thread per call and made every later call time out too, with no way
//! back: a strictly increasing backlog. `license::require` runs on every
//! clipboard capture, so a user whose vault is slow could accumulate threads
//! faster than they were reclaimed.
//!
//! One long-lived worker fixes both. A call still has a deadline, so a wedged
//! store is reported rather than hung on — but the deadline now costs a dropped
//! reply instead of an orphaned thread holding a lock. When the store recovers
//! the worker drains its queue and everything resumes.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

use zeroize::Zeroize;

/// The service every entry lives under, so there is exactly one place that
/// knows it.
pub const SERVICE: &str = "com.miisodev.bard";

/// The outcome of a credential read.
///
/// The distinction between "there is nothing stored" and "I could not find out"
/// is the whole point of this type. A caller that intends to create a value must
/// never treat a timeout as permission to overwrite what is already there.
pub enum Read {
    /// The entry exists and held a non-empty value.
    Value(String),
    /// The store answered: there is no entry, or the entry is empty.
    Absent,
    /// The store did not answer in time, or failed. Never treat this as absent.
    Unavailable,
}

/// The outcome of a credential write or delete.
pub enum Done {
    Ok,
    Unavailable,
}

enum Request {
    Read(String, Sender<Read>, Caller),
    Write(String, String, Sender<Done>, Caller),
    Delete(String, Sender<Done>, Caller),
}

/// Whether the caller is still waiting.
///
/// A caller that hit its deadline sets this, and from that moment a queued write
/// is one the caller has already decided did not happen. Serving it anyway is not
/// a harmless late completion: the trial writes its secret and its MAC as two
/// separate entries, so a first write landing after the caller gave up leaves the
/// pair half-written and the install permanently unable to verify its own trial,
/// with no in-product recovery.
struct Caller(Arc<AtomicBool>);

impl Caller {
    fn new() -> (Self, Arc<AtomicBool>) {
        let flag = Arc::new(AtomicBool::new(false));
        (Self(flag.clone()), flag)
    }

    fn gone(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

impl Request {
    /// Whether the caller has already given up on this request.
    fn gone(&self) -> bool {
        match self {
            Self::Read(_, _, caller)
            | Self::Write(_, _, _, caller)
            | Self::Delete(_, _, caller) => caller.gone(),
        }
    }

    /// Tell the caller the store is unavailable, for a panicking or dropped
    /// request, rather than leaving it to time out.
    fn reply_unavailable(&self) {
        match self {
            Self::Read(_, reply, _) => {
                let _ = reply.send(Read::Unavailable);
            }
            Self::Write(_, _, reply, _) | Self::Delete(_, reply, _) => {
                let _ = reply.send(Done::Unavailable);
            }
        }
    }

    /// Wipe a secret the worker is not going to write.
    fn zeroise(&mut self) {
        if let Self::Write(_, value, _, _) = self {
            value.zeroize();
        }
    }
}

fn worker() -> &'static Sender<Request> {
    static SENDER: OnceLock<Sender<Request>> = OnceLock::new();
    SENDER.get_or_init(|| {
        let (sender, receiver) = mpsc::channel::<Request>();
        // Detached on purpose: a credential read must not be able to keep the
        // process alive, and process exit is the only shutdown it needs.
        thread::spawn(move || {
            while let Ok(mut request) = receiver.recv() {
                // Never act on a request whose caller has given up. The caller
                // only knows its *reply* did not arrive; the request is still
                // queued and would otherwise execute whenever the store recovers,
                // long after the caller compensated for what it believed had
                // failed. For a write that is a correctness problem, not a
                // lateness one.
                if request.gone() {
                    request.zeroise();
                    continue;
                }
                // A panic inside the keyring crate must not take the worker with
                // it, or every later credential call would block until it timed
                // out and the failure would look like a wedged vault rather than a
                // bug. If `serve` panics, tell the caller the store is
                // unavailable rather than leaving it to time out.
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    serve(&request)
                }));
                if outcome.is_err() {
                    request.reply_unavailable();
                }
                // The worker holds the only copy of a written value, so this is
                // the one place it can be wiped. Doing it here rather than in
                // `serve` keeps `serve` borrow-only.
                request.zeroise();
            }
        });
        sender
    })
}

fn serve(request: &Request) {
    match request {
        Request::Read(username, reply, _) => {
            let outcome = match keyring::Entry::new(SERVICE, username) {
                Ok(entry) => match entry.get_password() {
                    Ok(value) if !value.trim().is_empty() => Read::Value(value),
                    // An empty password is an entry holding nothing, which for
                    // every caller here is the same as no entry.
                    Ok(_) | Err(keyring::Error::NoEntry) => Read::Absent,
                    Err(_) => Read::Unavailable,
                },
                Err(_) => Read::Unavailable,
            };
            let _ = reply.send(outcome);
        }
        Request::Write(username, value, reply, _) => {
            let outcome = match keyring::Entry::new(SERVICE, username) {
                Ok(entry) => match entry.set_password(value.trim()) {
                    Ok(()) => Done::Ok,
                    Err(_) => Done::Unavailable,
                },
                Err(_) => Done::Unavailable,
            };
            let _ = reply.send(outcome);
        }
        Request::Delete(username, reply, _) => {
            let outcome = match keyring::Entry::new(SERVICE, username) {
                Ok(entry) => match entry.delete_credential() {
                    // Deleting something already gone is the outcome asked for.
                    Ok(()) | Err(keyring::Error::NoEntry) => Done::Ok,
                    Err(_) => Done::Unavailable,
                },
                Err(_) => Done::Unavailable,
            };
            let _ = reply.send(outcome);
        }
    }
}

/// Send one request and wait for its reply, up to `timeout`.
///
/// A dropped or expired reply is indistinguishable from an unresponsive store at
/// this layer, and every caller treats it the same way, so collapsing the two
/// here is what the type system needs to route three differently-typed replies
/// through one queue. The worker closes the channel only when the process is
/// going away.
fn ask<T>(build: impl FnOnce(Sender<T>, Caller) -> Request, timeout: Duration) -> Option<T> {
    let (reply, answers) = mpsc::channel();
    let (caller, abandoned) = Caller::new();
    // The request owns the only copy of a secret, and `build` has already
    // constructed it by the time `send` is called. A failed send hands the request
    // back rather than consuming it, so it has to be wiped here — the worker's
    // wipe will never run for a request it never received.
    if let Err(unreachable) = worker().send(build(reply, caller)) {
        let mut orphan = unreachable.0;
        orphan.zeroise();
        abandoned.store(true, Ordering::Release);
        return None;
    }
    match answers.recv_timeout(timeout) {
        Ok(value) => Some(value),
        Err(_) => {
            // Tell the worker, if it has not already taken this one, that nobody
            // is waiting. Without this a queued write executes whenever the store
            // recovers, which is how a half-written trial pair becomes permanent.
            abandoned.store(true, Ordering::Release);
            None
        }
    }
}

/// Read one credential, distinguishing absence from unavailability.
pub fn read(username: &str, timeout: Duration) -> Read {
    let username = username.to_owned();
    ask(
        move |reply, caller| Request::Read(username, reply, caller),
        timeout,
    )
    .unwrap_or(Read::Unavailable)
}

/// Write one credential. The worker zeroises the value once the write finishes.
pub fn write(username: &str, value: String, timeout: Duration) -> Done {
    let username = username.to_owned();
    ask(
        move |reply, caller| Request::Write(username, value, reply, caller),
        timeout,
    )
    .unwrap_or(Done::Unavailable)
}

/// Delete one credential. Already-absent counts as success.
pub fn delete(username: &str, timeout: Duration) -> Done {
    let username = username.to_owned();
    ask(
        move |reply, caller| Request::Delete(username, reply, caller),
        timeout,
    )
    .unwrap_or(Done::Unavailable)
}
