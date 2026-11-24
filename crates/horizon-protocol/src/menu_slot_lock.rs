//! A locked inventory slot of an open menu: what a Horizon server tells a Horizon client
//! after a native mod locks the slot holding the item its menu was opened from
//! (`MenuRef::lock_inventory_slot`; AE2's `AEBaseMenu.lockPlayerInventorySlot`), so the
//! client's prediction takes nothing out of that slot and puts nothing in it. A vanilla
//! client ignores the payload; the server's refusal and resync still hold it.
//!
//! A payload is the window id (a varint), then whether a slot is locked (a boolean) and that
//! slot's inventory index (a byte: 0 to 35, or 40 for the offhand). Clientbound only.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
};

/// The custom payload channel.
pub const CHANNEL: &str = "horizon:menu_slot_lock";

/// The lock of window `window`'s menu: the inventory slot, None for none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lock {
    pub window: i32,
    pub slot: Option<u8>,
}

/// A lock's payload.
pub fn encode(lock: Lock) -> Vec<u8> {
    let mut w = Writer::new();
    w.varint(lock.window);
    w.bool(lock.slot.is_some());
    if let Some(slot) = lock.slot {
        w.u8(slot);
    }
    w.data
}

/// A payload's lock; an error for one that is cut, names no inventory slot, or has trailing
/// bytes.
pub fn decode(data: &[u8]) -> Result<Lock> {
    let mut r = Reader::new(data);
    let window = r.varint()?;
    let slot = if r.bool()? {
        let slot = r.u8()?;
        if !(slot < 36 || slot == 40) {
            return Err(Error::new("menu slot lock: not an inventory slot"));
        }
        Some(slot)
    } else {
        None
    };
    if r.remaining() > 0 {
        return Err(Error::new("menu slot lock: trailing bytes"));
    }
    Ok(Lock { window, slot })
}
