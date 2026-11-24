//! A mod message retried while its sender is in another level runs in their turn there, with the
//! outer body kept in its own level (`for_player`'s order).
use crate::game::Dim;
use crate::game::native_guest_turn_tests::with_guests;
use horizon_client::raw_payloads::{Classified, Delivery, Endpoint, Incoming, Side, Transport};
use mod_api::{
    Hooks, IPayloadContext, IPayloadHandler, ModEntry, PayloadBytes, PayloadId, PlayerRef,
    Registrar,
};
use std::sync::{Arc, Mutex};

const CHANNEL: &str = "retry_probe:message";

/// What the handler saw: the acting player and the level in place.
static SEEN: Mutex<Vec<(Option<PlayerRef>, &'static str)>> = Mutex::new(Vec::new());

struct Probe;
impl IPayloadHandler for Probe {
    fn handle(
        &self,
        _: PayloadBytes,
        context: &mut dyn IPayloadContext,
    ) -> Result<(), mod_api::PayloadHandleError> {
        let host = context
            .server_host()
            .map_err(|e| mod_api::PayloadHandleError::Failed(format!("{e:?}")))?;
        SEEN.lock()
            .unwrap()
            .push((host.players().acting_player(), host.dimension()));
        Ok(())
    }
}
#[mod_api::__subscriptions]
impl Hooks for Probe {
    fn register(&self, registrar: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
        registrar
            .payload_handlers()
            .registrar("1")
            .play_to_server(PayloadId(CHANNEL.into()), Arc::new(Probe));
    }
}

/// Guest 12 stands in the Nether; the Overworld is in place, as on the first level pass that
/// retries a deferred message.
fn guest_in_the_nether(game: &mut crate::game::Game<'_>) {
    assert_eq!(game.dim, Dim::Overworld);
    let gi = game.guest_index(12).unwrap();
    game.lan
        .as_mut()
        .unwrap()
        .hub
        .guests
        .update(gi, |g| g.dimension = Dim::Nether.id().to_owned());
}

#[test]
#[ignore = "daily"]
fn a_message_from_another_level_runs_in_the_senders_turn_there() {
    let mods = vec![ModEntry::new("retry_probe", Box::new(Probe))].into();
    mod_platform::testing::with_mods(mods, || {
        with_guests(|game| {
            guest_in_the_nether(game);
            let transport = Transport::capture(Side::Server).unwrap();
            assert!(
                transport.local.owns(CHANNEL),
                "the probe's channel is registered"
            );
            transport
                .accept(transport.local.manifest.entries())
                .unwrap();
            transport.finish_configuration().unwrap();
            let Classified::Application(incoming) = Incoming::classify(
                &transport,
                horizon_protocol::payload_negotiation::Phase::Play,
                horizon_protocol::packets::configuration::CustomPayload {
                    channel: CHANNEL.into(),
                    data: Vec::new(),
                },
            )
            .unwrap() else {
                panic!("the probe's channel classifies as an application payload");
            };
            let sender = game
                .lan
                .as_ref()
                .unwrap()
                .hub
                .guest(12)
                .unwrap()
                .sender
                .clone();
            game.receive_raw_delivery(Box::new(Delivery {
                conn: Some(12),
                endpoint: Endpoint::server(sender),
                incoming,
            }));
            let seen = SEEN.lock().unwrap().clone();
            assert_eq!(seen.len(), 1, "the handler ran once");
            assert!(seen[0].0.is_some(), "the sender is the acting player");
            assert_eq!(seen[0].1, Dim::Nether.id(), "in the sender's level");
            assert_eq!(game.dim, Dim::Overworld, "the outer level is back");
        });
    });
}

/// The path `receive_raw_delivery` takes: guest 13 (the outer turn) stands in the Overworld, guest
/// 12 sends from the Nether. In 12's turn there, `isUnobstructed` sees 13's body in the Overworld
/// and none in the Nether (`for_player`'s order).
#[test]
#[ignore = "daily"]
fn the_outer_body_stays_in_its_own_level_during_the_turn() {
    with_guests(|game| {
        guest_in_the_nether(game);
        let (in_the_nether, in_the_overworld) = game
            .as_guest(13, |g| {
                g.as_payload_sender(12, |g| {
                    (
                        g.lan_building_boxes.boxes(Dim::Nether),
                        g.lan_building_boxes.boxes(Dim::Overworld),
                    )
                })
            })
            .unwrap()
            .expect("12's turn runs inside 13's");
        assert_eq!(in_the_nether, Vec::new(), "no body in the Nether");
        assert_eq!(in_the_overworld.len(), 1, "13's body in the Overworld");
        assert_eq!(game.dim, Dim::Overworld, "the outer level is back");
    });
}
