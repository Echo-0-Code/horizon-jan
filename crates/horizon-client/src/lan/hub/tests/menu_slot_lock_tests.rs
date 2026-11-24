//! A mod's native menu opened from an item locks the inventory slot holding it
//! (`MenuRef::lock_inventory_slot`, AE2's `AEBaseMenu.lockPlayerInventorySlot`): a click that
//! would move that slot's stack never reaches the mod, the others still do, and the lock
//! ends with the menu.
use super::*;
use crate::lan::menu::{Menu, NativeMenuEvent, Slots};

/// A blockless mod menu of 27 slots, window `window`, as `open_mod_menu` keeps it.
fn native_menu(window: i32) -> Menu {
    Menu {
        window,
        kind: 0,
        pos: None,
        slots: Slots::Native,
        size: 27,
        state: 0,
        sent: Vec::new(),
        drag: None,
        sent_data: Vec::new(),
        hidden: Vec::new(),
        mod_type: None,
        registry_id: "example:terminal".into(),
        craft: None,
        links: Vec::new(),
        asks_still_valid: false,
    }
}

fn click(slot: i16, button: i8, input: i32) -> play::ContainerClick {
    play::ContainerClick {
        container: 1,
        state: 0,
        slot,
        button,
        input,
        changed: Vec::new(),
        carried: None,
    }
}

/// The `horizon:menu_slot_lock` payloads the guest was sent since the last drain.
fn locks_sent(
    rx: &std::sync::mpsc::Receiver<crate::lan::Out>,
) -> Vec<horizon_protocol::menu_slot_lock::Lock> {
    let mut bodies = Vec::new();
    while let Ok(out) = rx.try_recv() {
        match out {
            crate::lan::Out::Packet(body) => bodies.push(body),
            crate::lan::Out::Packets(batch) => bodies.extend(batch),
            crate::lan::Out::Disconnect(_) => {}
        }
    }
    bodies
        .iter()
        .filter_map(|body| {
            let mut reader = horizon_protocol::buf::Reader::new(body);
            let id = reader.varint().ok()?;
            match C::decode(id, reader.rest()).ok()? {
                C::CustomPayload(p) if p.channel == horizon_protocol::menu_slot_lock::CHANNEL => {
                    horizon_protocol::menu_slot_lock::decode(&p.data).ok()
                }
                _ => None,
            }
        })
        .collect()
}

#[test]
fn a_locked_slot_takes_no_click_and_the_lock_ends_with_the_menu() {
    let (mut hub, rx) = hub_with_a_guest();
    let world = World::new(horizon_world::registry::BlockStateId(0), 0);
    let conn = hub.guests[0].conn;
    hub.guests.update(0, |g| {
        g.inventory[0] = Some(ItemStack::new("minecraft:compass", 1));
        g.inventory[1] = Some(ItemStack::new("minecraft:stone", 8));
        g.native_menu = vec![None; 27];
        g.menu = Some(native_menu(1));
    });
    let _ = locks_sent(&rx);

    // Only an inventory slot of the open native menu locks.
    assert!(
        !hub.lock_native_menu_slot(conn, 2, Some(0)),
        "another window"
    );
    assert!(
        !hub.lock_native_menu_slot(conn, 1, Some(36)),
        "an armour slot"
    );
    assert!(hub.lock_native_menu_slot(conn, 1, Some(0)));
    assert_eq!(hub.native_menu_lock(conn, 1), Some(0));
    assert_eq!(
        locks_sent(&rx),
        [horizon_protocol::menu_slot_lock::Lock {
            window: 1,
            slot: Some(0)
        }],
        "the client is told, so its prediction leaves the slot alone"
    );

    // The menu's 27 slots, then the inventory's 27, then the hotbar's: hotbar 0 is slot 54.
    let refused = [
        click(54, 0, 0), // pick it up
        click(54, 0, 1), // shift-click it
        click(54, 1, 4), // throw it
        click(27, 0, 2), // number key 1 over another slot
        click(54, 1, 2), // number key 2 over it
    ];
    for p in &refused {
        assert!(hub.prepare_click(conn, p, &world).is_none(), "{p:?}");
    }
    assert!(
        hub.native_menu_events.is_empty(),
        "no refused click reaches the mod"
    );
    // The other slots and keys still go to the mod.
    for p in [click(55, 0, 0), click(27, 1, 2), click(0, 0, 0)] {
        assert!(hub.prepare_click(conn, &p, &world).is_none());
    }
    let heard: Vec<(i32, i32, i32)> = hub
        .native_menu_events
        .drain(..)
        .filter_map(|e| match e {
            NativeMenuEvent::Click {
                slot, button, mode, ..
            } => Some((slot, button, mode)),
            _ => None,
        })
        .collect();
    assert_eq!(heard, [(55, 0, 0), (27, 1, 2), (0, 0, 0)]);

    // Unlocked, the slot takes clicks again.
    assert!(hub.lock_native_menu_slot(conn, 1, None));
    assert!(hub.prepare_click(conn, &click(54, 0, 0), &world).is_none());
    assert_eq!(hub.native_menu_events.len(), 1);
    hub.native_menu_events.clear();

    // A closed menu takes its lock with it: the next window of that id starts unlocked.
    assert!(hub.lock_native_menu_slot(conn, 1, Some(0)));
    hub.close_container(conn);
    hub.guests.update(0, |g| g.menu = Some(native_menu(1)));
    assert_eq!(hub.native_menu_lock(conn, 1), None);
}
