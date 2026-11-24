//! `ServerGamePacketListenerImpl.handleCustomPayload` runs on the server thread for the sending
//! `ServerPlayer`: a mod's handler for a client's message acts as that player, as a packet from
//! them does. It names them as the acting player, can send them into another level, and what it
//! tells them reaches their chat.
use mod_api::{
    Hooks, IPayloadContext, IPayloadHandler, ModEntry, PayloadBytes, PayloadId, Registrar, Text,
};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const NAME: &str = "game::payload_transport::turn_tests::a_client_message_runs_in_the_senders_turn";
const CHANNEL: &str = "turn_probe:warp";
const TOLD: &str = "turn probe: warped";

/// What the server handler saw: whether the sender was the acting player, whether the jump into
/// the Nether was taken, and whether the message was sent.
static SEEN: Mutex<Vec<(bool, bool, bool)>> = Mutex::new(Vec::new());

struct Warp;
impl IPayloadHandler for Warp {
    fn handle(
        &self,
        _: PayloadBytes,
        context: &mut dyn IPayloadContext,
    ) -> Result<(), mod_api::PayloadHandleError> {
        let failed = |e| mod_api::PayloadHandleError::Failed(format!("{e:?}"));
        let player = context.player().map_err(failed)?;
        let host = context.server_host().map_err(failed)?;
        let acting = host.players().acting_player() == Some(player);
        let warped =
            player
                .entity()
                .teleport_to_level(host, "minecraft:the_nether", [0.5, 70.0, 0.5], None);
        let told = player.tell(host, Text::literal(TOLD)) == Ok(true);
        SEEN.lock().unwrap().push((acting, warped, told));
        Ok(())
    }
}

struct Registration;
#[mod_api::__subscriptions]
impl Hooks for Registration {
    fn register(&self, registrar: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
        registrar
            .payload_handlers()
            .registrar("1")
            .play_to_server(PayloadId(CHANNEL.into()), Arc::new(Warp));
    }
}

/// Single player, so the sender is the integrated server's owner over memory: the same route a
/// LAN guest's or a dedicated server's player's message takes once it reaches the hub.
#[test]
#[ignore = "daily"]
fn a_client_message_runs_in_the_senders_turn() {
    super::tests::require_resources();
    if super::super::native_entity_refs::tests::run_fresh_cases(
        NAME,
        "HORIZON_PAYLOAD_TURN_CHILD",
        &["integrated"],
    ) {
        return;
    }
    let mods = vec![ModEntry::new("turn_probe", Box::new(Registration))].into();
    mod_platform::testing::with_mods(mods, || {
        super::super::native_entity_refs::tests::with_client_game(|game| {
            game.settings.render_distance = 2;
            game.settings.simulation_distance = 2;
            let options = horizon_client::title::WorldOptions {
                world_type: horizon_client::save::WorldPreset::Flat,
                ..Default::default()
            };
            game.open_integrated("turn", Some("Turn"), Some(1), true, Some(&options))
                .unwrap();
            let ready = |game: &super::super::Game<'_>| {
                game.remote
                    .as_ref()
                    .is_some_and(|r| r.logged_in && r.joined)
            };
            drive(game, "integrated play ready", ready);
            assert_eq!(game.dim.id(), "minecraft:overworld");
            let host: &mut dyn mod_api::ClientHost = game;
            assert_eq!(
                host.send_payload_to_server(&PayloadId(CHANNEL.into()), PayloadBytes(Vec::new())),
                Ok(mod_api::PayloadAccepted::Queued)
            );
            drive(game, "in the Nether and told", |game| {
                game.dim.id() == "minecraft:the_nether"
                    && game.ui.chat.recent_lines(10).iter().any(|l| l == TOLD)
            });
            assert_eq!(*SEEN.lock().unwrap(), [(true, true, true)]);
            game.leave_server(None).unwrap();
        });
    });
}

fn drive(
    game: &mut super::super::Game<'_>,
    what: &str,
    done: impl Fn(&super::super::Game<'_>) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !done(game) {
        game.poll_remote().unwrap();
        assert!(
            Instant::now() < deadline,
            "{what}; the server handler saw {:?}",
            SEEN.lock().unwrap()
        );
        std::thread::yield_now();
    }
}
