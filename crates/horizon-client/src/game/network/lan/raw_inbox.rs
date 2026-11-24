//! Reborrow live world state only after a raw root has completed.
use super::*;

impl Game<'_> {
    pub(super) fn resume_raw_inbox(
        &mut self,
        hub: &mut Hub,
        entities: &[horizon_client::lan::entity::EntityState],
        spawn: [f64; 3],
    ) -> Vec<HostEvent> {
        // Raw transactions finished above. Reborrow current world state only
        // now; no callback captures a TickCtx or an as_guest world swap.
        let clock_rate = self.network_clock_rate();
        let mut ctx = TickCtx {
            sea_level: self.end.locator().map_or(63, |terrain| terrain.sea_level()),
            world: &mut self.world,
            entities,
            next_entity: &mut self.next_entity,
            spawn,
            spawn_angle: self.level.world_spawn_angle,
            spawn_dimension: self.level.spawn_dimension(),
            difficulty: i32::from(self.ui.difficulty),
            difficulty_locked: self.level.difficulty_locked,
            clock_rate,
            tick_rate: self.clock.rate(),
            tick_frozen: self.clock.frozen(),
            tick_steps: self.clock.step_state(),
            collision: Some(self.collision),
            border: &self.border,
            streaming: Some(&self.streaming),
        };
        hub.drain_packets(&mut ctx)
    }
}

impl Game<'_> {
    pub(super) fn drain_raw_turns(
        &mut self,
        mut host: LanHost,
        mut events: Vec<HostEvent>,
        entities: &[horizon_client::lan::entity::EntityState],
        spawn: [f64; 3],
    ) -> Option<LanHost> {
        loop {
            let back = horizon_client::spans::time("lan_events", || {
                self.lan_process_events(host, events, entities, spawn)
            })?;
            host = back;
            if !host.hub.packets_paused() {
                return Some(host);
            }
            events = self.resume_raw_inbox(&mut host.hub, entities, spawn);
        }
    }
}

impl Game<'_> {
    pub(super) fn receive_raw_delivery(
        &mut self,
        mut delivery: Box<horizon_client::raw_payloads::Delivery>,
    ) {
        // `ServerGamePacketListenerImpl.handleCustomPayload` runs on the server thread for
        // the sending `ServerPlayer`: a handler acts as that player, in their level, as a
        // packet from them does. A delivery with no player in the world (configuration, a
        // guest not placed yet, one already acting) keeps the plain server context.
        let mut turn = None;
        if let Some(conn) = delivery.conn
            && let Some(theirs) = self.payload_turn(conn)
        {
            // A first delivery is drained in the sender's own level pass, so their level is
            // already in place and the turn costs what a packet's does. Only a delivery a busy
            // handler deferred is retried from the first pass; for it their level is put in
            // place (a level swap, once per retry), as a leaving player's turn does, and the
            // outer body is told it stays in `back` (`for_player`'s order).
            let back = self.dim;
            let entered = theirs == back
                || match self.enter_level(theirs) {
                    Ok(()) => true,
                    Err(e) => {
                        log::error!("could not open {}: {e}", theirs.id());
                        false
                    }
                };
            // A handler's panic goes on only once the outer level is back in place.
            let mut unwound = Ok(());
            if entered {
                let endpoint = delivery.endpoint.clone();
                let incoming = &mut delivery.incoming;
                unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.as_guest_messaged_from(conn, back, |game| {
                        turn = Some(game.receive_server_payload(endpoint, incoming));
                    });
                }));
            }
            if self.dim != back
                && let Err(e) = self.enter_level(back)
            {
                log::error!("could not reopen {}: {e}", back.id());
            }
            if let Err(payload) = unwound {
                std::panic::resume_unwind(payload);
            }
        }
        let finished = match turn {
            Some(finished) => finished,
            None => self.receive_server_payload(delivery.endpoint.clone(), &mut delivery.incoming),
        };
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        if !finished {
            host.hub.defer_raw_payload(delivery);
        } else {
            host.hub.finish_raw_payload(delivery.conn);
        }
    }
}

impl Game<'_> {
    /// The level of the player on connection `conn` when a payload from them can run in their
    /// turn: they are in the world, and not already the player acting.
    fn payload_turn(&self, conn: u32) -> Option<crate::game::Dim> {
        let guest = self.lan.as_ref()?.hub.guest(conn)?;
        (guest.placed && !self.lan_guest_swaps.contains(&conn))
            .then(|| crate::game::Dim::from_id(&guest.dimension))
    }
}

#[cfg(test)]
#[path = "raw_inbox_tests.rs"]
mod tests;
