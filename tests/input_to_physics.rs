//! End-to-end input plumbing: a synthetic terminal key stream folded by
//! [`InputCollector`] and fed straight into the physics step. No terminal is
//! initialised anywhere here; only the timing of the events is real.
//!
//! This is the seam the unit tests on either side cannot cover. An inferred
//! hold that lapses mid-flight is invisible in the air — the direction is still
//! at the cap and `air_decel` is gentle — and only shows up as a speed crash on
//! landing, once the far harsher `ground_decel` takes over.

use shelljump::entities::Player;
use shelljump::input::{GameKey, HoldMode, InputCollector, InputState};
use shelljump::math::Vec2;
use shelljump::physics::{PhysicsConfig, step_player};
use shelljump::time::FIXED_DT;
use shelljump::world::{Tile, TileMap};

/// Typical OS "delay until repeat" before a held key's first auto-repeat.
const INITIAL_DELAY: f64 = 0.5;
/// Typical gap between auto-repeats once the stream is running.
const REPEAT_INTERVAL: f64 = 0.03;

fn ground_map(ground_row: usize) -> TileMap {
    let mut map = TileMap::new(256, ground_row + 4);
    for x in 0..256 {
        map.set(x, ground_row, Tile::Solid);
    }
    map
}

/// Drives the collector and the simulation off one shared clock.
struct Rig {
    collector: InputCollector,
    player: Player,
    map: TileMap,
    cfg: PhysicsConfig,
    now: f64,
    /// When the next synthetic Right auto-repeat is due, or `None` once the
    /// terminal has stopped delivering events for that key.
    next_right: Option<f64>,
    right_gap: f64,
}

impl Rig {
    fn new(ground_row: usize, spawn: Vec2) -> Self {
        Self {
            collector: InputCollector::new(HoldMode::Timeout),
            player: Player::new(spawn),
            map: ground_map(ground_row),
            cfg: PhysicsConfig::default(),
            now: 0.0,
            next_right: Some(0.0),
            right_gap: INITIAL_DELAY,
        }
    }

    fn step(&mut self) {
        if let Some(due) = self.next_right
            && self.now >= due
        {
            self.collector.on_press(GameKey::Right, self.now);
            self.next_right = Some(self.now + self.right_gap);
            self.right_gap = REPEAT_INTERVAL;
        }

        let state = self.collector.finish_frame(self.now);
        step_player(&mut self.player, &self.map, &state, &self.cfg, FIXED_DT);
        self.collector.acknowledge_edges();
        self.now += f64::from(FIXED_DT);
    }

    /// Runs until the horizontal cap is reached, so every test starts from the
    /// same well-defined speed.
    fn accelerate_to_the_walk_cap(&mut self) {
        for _ in 0..2000 {
            self.step();
            if self.player.velocity.x >= self.cfg.max_walk_speed - 1e-3 {
                return;
            }
        }
        panic!("never reached the walk cap");
    }

    /// Taps Jump once and cuts Right's event stream dead. The player is still
    /// physically holding Right; the terminal simply never resumes its repeats.
    fn tap_jump_and_starve_the_direction(&mut self) {
        self.collector.on_press(GameKey::Jump, self.now);
        self.next_right = None;
    }
}

#[test]
fn a_held_direction_survives_a_whole_jump_with_no_further_repeats() {
    let mut rig = Rig::new(20, Vec2::new(4.0, 20.0 - 0.45));
    rig.player.grounded = true;
    rig.accelerate_to_the_walk_cap();
    assert!(rig.player.grounded, "the jump must start from the ground");

    let cap = rig.cfg.max_walk_speed;
    rig.tap_jump_and_starve_the_direction();
    let takeoff = rig.now;

    let mut slowest = rig.player.velocity.x;
    let mut worst_drop = 0.0f32;
    let mut airborne = false;
    let mut landed = false;
    for _ in 0..2000 {
        let before = rig.player.velocity.x;
        rig.step();
        worst_drop = worst_drop.max(before - rig.player.velocity.x);
        slowest = slowest.min(rig.player.velocity.x);
        if rig.player.grounded {
            if airborne {
                landed = true;
                break;
            }
        } else {
            airborne = true;
        }
    }

    assert!(landed, "the jump never came back down");
    let flight = rig.now - takeoff;
    assert!(
        flight > 1.0,
        "a flight of {flight}s is too short to be a jump"
    );
    assert!(
        slowest >= cap * 0.9,
        "kept only {} of the walk cap across a {flight}s flight",
        slowest / cap
    );
    assert!(
        worst_drop < rig.cfg.ground_decel * FIXED_DT * 0.5,
        "a single step shed {worst_drop}, the signature of ground_decel taking \
         over after the direction was wrongly written off"
    );
}

/// In `HoldMode::Timeout` a genuinely held Jump key can latch a second raw
/// `jump_pressed` edge: its first auto-repeat arrives one OS initial delay after
/// the press, which is indistinguishable on timing alone from a deliberate
/// second tap. That is an accepted tradeoff, and this is the proof it costs
/// nothing in play. The extra edge lands mid-flight, by which time `coyote_time`
/// (0.1s) has long since run out, and it only refills `jump_buffer_time` (0.1s),
/// which expires while the player is still in the air. So it can never buy a
/// second launch — asserted here against the real simulation, not by argument.
#[test]
fn a_held_jump_key_launches_exactly_one_jump() {
    const GROUND_ROW: usize = 20;

    for initial_delay in [0.225, 0.375, 0.50, 0.66] {
        for interval in [0.03, 0.05, 0.09] {
            let map = ground_map(GROUND_ROW);
            let cfg = PhysicsConfig::default();
            let mut player = Player::new(Vec2::new(4.0, GROUND_ROW as f32 - 0.45));
            let mut collector = InputCollector::new(HoldMode::Timeout);

            for _ in 0..20 {
                step_player(&mut player, &map, &InputState::default(), &cfg, FIXED_DT);
            }
            assert!(player.grounded, "the jump must start from the ground");

            let mut now = 0.0;
            let mut next_event = 0.0;
            let mut gap = initial_delay;
            let mut raw_edges = 0;
            let mut launches = 0;
            let mut airborne_for = 0.0f64;
            while now < 2.0 {
                if now >= next_event {
                    collector.on_press(GameKey::Jump, now);
                    next_event = now + gap;
                    gap = interval;
                }
                let state = collector.finish_frame(now);
                if state.jump_pressed {
                    raw_edges += 1;
                }
                let was_grounded = player.grounded;
                step_player(&mut player, &map, &state, &cfg, FIXED_DT);
                // The floor is unbroken, so the only way off it is a launch.
                if was_grounded && !player.grounded {
                    launches += 1;
                }
                if !player.grounded {
                    airborne_for += f64::from(FIXED_DT);
                }
                collector.acknowledge_edges();
                now += f64::from(FIXED_DT);
            }

            let case = format!("delay {initial_delay}, interval {interval}");
            assert!(
                raw_edges <= 2,
                "{case}: {raw_edges} raw edges from one hold"
            );
            assert_eq!(launches, 1, "{case}: a held key must not bunny-hop");
            assert!(
                airborne_for > 1.0,
                "{case}: only {airborne_for}s airborne, too short to be a jump"
            );
            assert!(
                player.grounded,
                "{case}: the jump never came back down, so nothing was proven \
                 about a second launch"
            );
        }
    }
}

/// The discriminator for the test above: same rig, same starved event stream,
/// but the body stays in the air long enough for the inferred hold to genuinely
/// lapse. If the assertions there had no teeth, this would pass them too.
#[test]
fn the_rig_still_detects_a_direction_that_really_does_expire() {
    let mut rig = Rig::new(200, Vec2::new(4.0, 2.0));
    rig.accelerate_to_the_walk_cap();
    assert!(!rig.player.grounded, "this variant must stay airborne");

    let cap = rig.cfg.max_walk_speed;
    rig.tap_jump_and_starve_the_direction();

    let mut slowest = rig.player.velocity.x;
    for _ in 0..2000 {
        rig.step();
        slowest = slowest.min(rig.player.velocity.x);
        if rig.player.grounded {
            break;
        }
    }

    assert!(rig.player.grounded, "the fall never reached the floor");
    assert!(
        slowest < cap * 0.9,
        "total silence must eventually be read as a release, kept {}",
        slowest / cap
    );
}
