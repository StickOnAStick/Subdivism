pub mod authority;
pub mod net;
pub mod session;
pub mod snapshot;

use std::time::{Duration, Instant};

use crate::{
    game::{physics::PhysicsConfig, world::World},
    server::session::ServerSession,
    shared::sim::step,
};

pub fn run_headless() {
    let world = World::generate_default();
    let physics = PhysicsConfig::default();
    let mut session = ServerSession::new();
    let tick_hz = step::DEFAULT_SIM_TICK_HZ;
    let tick_dt = Duration::from_secs_f32(step::fixed_dt_seconds(tick_hz));

    let start = Instant::now();
    let mut ticks = 0_u32;
    while ticks < 120 {
        session.tick(&world, &physics, tick_dt.as_secs_f32());
        ticks += 1;
    }

    let elapsed_ms = start.elapsed().as_millis();
    println!(
        "subdivism server stub: {} ticks @ {}hz in {}ms",
        ticks, tick_hz, elapsed_ms
    );
}

pub fn run_dedicated_udp(bind_addr: &str) {
    net::run_udp_server(bind_addr);
}
