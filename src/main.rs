use std::net::SocketAddr;
use bevy::color::Color;
use bevy::DefaultPlugins;
use bevy::prelude::*;
use bevy::sprite::{MaterialMesh2dBundle, Mesh2dHandle, Wireframe2dPlugin};
use bevy::time::Time;
use bevy::input::ButtonInput;
use bevy::utils::HashMap;
use bevy_ggrs::{AddRollbackCommandExtension, GgrsApp, GgrsConfig, GgrsPlugin, GgrsSchedule, LocalInputs, LocalPlayers, PlayerInputs, ReadInputs, Rollback, Session};
use bevy_ggrs::ggrs::{PlayerType, UdpNonBlockingSocket};
use bevy_ggrs::prelude::SessionBuilder;
use bytemuck::{Pod, Zeroable};
use clap::Parser;

const PLAYER_SPEED: f32 = 200.;
const UPS: f32 = 60.;
static SPU: f32 = 1. / UPS;


const INPUT_UP: u8 = 0;
const INPUT_DOWN: u8 = 1;
const INPUT_RIGHT: u8 = 2;
const INPUT_LEFT: u8 = 4;


#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Pod, Zeroable)]
struct InputPacked {
    wasd: u8
}
type Config = GgrsConfig<InputPacked>;

#[derive(Clone, Copy, Component)]
struct Velocity {
    x: f32,
    y: f32,
}

#[derive(Component)]
struct Player {
    id: usize
}

fn read_local_inputs(
    mut commands: Commands,
    input: Res<ButtonInput<KeyCode>>,
    local_players: Res<LocalPlayers>
) {
    let mut local_inputs = HashMap::new();

    for id in &local_players.0 {
        let wasd: u8 =
            (input.pressed(KeyCode::ArrowUp) as u8) |
            ((input.pressed(KeyCode::ArrowDown) as u8) << 1u8) |
            ((input.pressed(KeyCode::ArrowRight) as u8) << 2u8) |
            ((input.pressed(KeyCode::ArrowLeft) as u8) << 3u8);
        local_inputs.insert(*id, InputPacked{wasd});
    }

    commands.insert_resource(LocalInputs::<Config>(local_inputs));
}

fn handle_players(
    mut query: Query<(&mut Velocity, &Player), With<Rollback>>,
    inputs: Res<PlayerInputs<Config>>
) {
    for (mut vel, player) in query.iter_mut() {
        let wasd = inputs[player.id].0.wasd;

        vel.y = (((wasd >> 0) & 1) as i32 - ((wasd >> 1) & 1) as i32) as f32 * PLAYER_SPEED;
        vel.x = (((wasd >> 2) & 1) as i32 - ((wasd >> 3) & 1) as i32) as f32 * PLAYER_SPEED;
    }
}

fn velocity_system(mut query: Query<(&mut Transform, &Velocity), With<Rollback>>) {
    for (mut transform, vel) in query.iter_mut() {
        transform.translation.x += vel.x * SPU;
        transform.translation.y += vel.y * SPU;
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    session: Res<Session<Config>>
) {
    let players_num = match &*session {
        Session::SyncTest(s) => s.num_players(),
        Session::P2P(s) => s.num_players(),
        Session::Spectator(s) => s.num_players(),
    };

    let mesh = meshes.add(Circle::new(25.));

    for i in 0..players_num {
        commands.spawn((
            MaterialMesh2dBundle {
                mesh: Mesh2dHandle(mesh.clone()),
                material: materials.add(Color::hsv(1., 1., 1. / (players_num * i) as f32)),
                transform: Transform::from_translation(Vec3{x: (100isize - (100isize * players_num as isize) + (200isize * i as isize)) as f32, y: 0., z: 0.}),
                ..default()
            },
            Velocity {
                x: 0.,
                y: 0.
            },
            Player {
              id: i
            }
            )).add_rollback();
    }

    commands.spawn(Camera2dBundle::default());
}

#[derive(Parser, Resource)]
struct Opt {
    #[clap(short, long)]
    local_port: u16,
    #[clap(short, long, num_args = 1..)]
    players: Vec<String>,
}

fn main() {
    let opt = Opt::parse();
    let players_num = opt.players.len();
    assert!(players_num > 0);

    let mut sess_build = SessionBuilder::<Config>::new()
        .with_num_players(players_num)
        // .with_desync_detection_mod(ggrs::DesyncDetection::On {interval: 10})
        .with_max_prediction_window(12)
        .unwrap();

    for (i, player_addr) in opt.players.iter().enumerate() {
        // local player
        if player_addr == "localhost" {
            sess_build = sess_build.add_player(PlayerType::Local, i).unwrap();
        } else {
            // remote players
            let remote_addr: SocketAddr = player_addr.parse().unwrap();
            sess_build = sess_build.add_player(PlayerType::Remote(remote_addr), i).unwrap();
        }
    }

    let socket = UdpNonBlockingSocket::bind_to_port(opt.local_port).unwrap();
    let sess = sess_build.start_p2p_session(socket).unwrap();

    App::new()
        .add_plugins((
            DefaultPlugins,
            Wireframe2dPlugin,
            GgrsPlugin::<Config>::default()
            ))
        .set_rollback_schedule_fps(UPS as usize)
        .rollback_component_with_clone::<Transform>()
        .rollback_component_with_copy::<Velocity>()
        .insert_resource(opt)
        .insert_resource(Session::P2P(sess))
        .insert_resource(Time::<Fixed>::from_hz(UPS as f64))
        .add_systems(Startup, setup)
        .add_systems(GgrsSchedule, (handle_players, velocity_system.after(handle_players)))
        .add_systems(ReadInputs, (read_local_inputs))
        .run();
}
