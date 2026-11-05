//! The table of native network handles (sockets, listeners, streams, UDP
//! packets). A handle is `(generation << 32) | slot`: closing a handle bumps
//! its slot's generation, so an id the program kept after `Close` never
//! reaches a socket opened later in the same slot. It answers `Net.CLOSED`,
//! as in `bni` (value-memory-abi.md: a stale handle returns a status, never
//! another object). The same packing as the ARC `ObjectId`.

use crate::net::{TcpListener, TcpStream, UdpPacket, UdpSocket};
use std::sync::{Arc, Mutex};

pub enum Handle {
    TcpStream(TcpStream),
    TcpListener(TcpListener),
    UdpSocket(UdpSocket),
    UdpPacket(UdpPacket),
}

/// Bits of a handle that hold the slot index; the generation is above them.
const SLOT_BITS: u32 = 32;
const SLOT_MASK: i64 = (1 << SLOT_BITS) - 1;
/// The first generation of a slot. Never 0, so no valid handle is 0.
const FIRST_GENERATION: i64 = 1;
/// A slot whose generation reaches this value is retired, never reused, so
/// the packed handle stays a positive `i64` and no generation repeats.
const RETIRED_GENERATION: i64 = i32::MAX as i64;

struct Slot {
    generation: i64,
    handle: Option<Handle>,
}

/// The sockets of the module copy that runs on this thread (`bn_core_copy`):
/// a task's close when the task ends.
#[derive(Default)]
struct Sockets(Mutex<Vec<Slot>>);

fn table() -> Arc<Sockets> {
    bn_core_copy::table::<Sockets>()
}

fn pack(index: usize, generation: i64) -> i64 {
    (generation << SLOT_BITS) | i64::try_from(index).unwrap_or(SLOT_MASK)
}

/// The slot of `id` when its generation is the slot's current one.
fn slot_of(slots: &[Slot], id: i64) -> Option<usize> {
    let index = usize::try_from(id & SLOT_MASK).ok()?;
    let generation = id >> SLOT_BITS;
    (slots.get(index)?.generation == generation).then_some(index)
}

pub fn insert(handle: Handle) -> Result<i64, &'static str> {
    let sockets = table();
    let mut slots = sockets
        .0
        .lock()
        .map_err(|_| "network handle table poisoned")?;
    if let Some((index, slot)) = slots
        .iter_mut()
        .enumerate()
        .find(|(_, slot)| slot.handle.is_none() && slot.generation < RETIRED_GENERATION)
    {
        slot.handle = Some(handle);
        return Ok(pack(index, slot.generation));
    }
    // The quota the interpreter applies too (`bn_limits`).
    if slots.len() >= bn_limits::web_limits().socket_handles_max {
        return Err("socket handle quota exceeded");
    }
    slots.push(Slot {
        generation: FIRST_GENERATION,
        handle: Some(handle),
    });
    Ok(pack(slots.len() - 1, FIRST_GENERATION))
}

/// Closes `id`: the handle leaves the table and its slot's generation moves
/// on. A stale or unknown id is `None` (close is idempotent).
pub fn remove(id: i64) -> Result<Option<Handle>, &'static str> {
    let sockets = table();
    let mut slots = sockets
        .0
        .lock()
        .map_err(|_| "network handle table poisoned")?;
    let Some(index) = slot_of(&slots, id) else {
        return Ok(None);
    };
    let slot = &mut slots[index];
    let handle = slot.handle.take();
    if handle.is_some() {
        slot.generation += 1;
    }
    Ok(handle)
}

pub fn with<T>(id: i64, operation: impl FnOnce(&Handle) -> T) -> Result<Option<T>, &'static str> {
    let sockets = table();
    let slots = sockets
        .0
        .lock()
        .map_err(|_| "network handle table poisoned")?;
    Ok(slot_of(&slots, id)
        .and_then(|index| slots[index].handle.as_ref())
        .map(operation))
}

pub(crate) fn with_mut<T>(
    id: i64,
    operation: impl FnOnce(&mut Handle) -> T,
) -> Result<Option<T>, &'static str> {
    let sockets = table();
    let mut slots = sockets
        .0
        .lock()
        .map_err(|_| "network handle table poisoned")?;
    Ok(match slot_of(&slots, id) {
        Some(index) => slots[index].handle.as_mut().map(operation),
        None => None,
    })
}

#[cfg(test)]
mod tests {
    use super::{Handle, insert, remove, with};
    use crate::net::{Address, Endpoint, UdpSocket};

    fn loopback_socket() -> UdpSocket {
        UdpSocket::bind(Endpoint::new(
            Address::parse("127.0.0.1").expect("loopback"),
            0,
        ))
        .expect("bind loopback")
    }

    #[test]
    fn a_closed_handle_never_reaches_the_socket_that_reuses_its_slot() {
        let _lock = crate::network_test_lock();
        let first = insert(Handle::UdpSocket(loopback_socket())).expect("insert handle");
        assert!(with(first, |_| ()).expect("lookup").is_some());
        assert!(remove(first).expect("close").is_some());
        let second = insert(Handle::UdpSocket(loopback_socket())).expect("insert again");
        assert_ne!(first, second, "a reused slot gets a new generation");
        assert!(with(first, |_| ()).expect("lookup stale").is_none());
        assert!(with(second, |_| ()).expect("lookup live").is_some());
        assert!(remove(first).expect("stale close").is_none());
        assert!(with(second, |_| ()).expect("still live").is_some());
        let _ = remove(second);
    }
}
