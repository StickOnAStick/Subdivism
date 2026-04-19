use glam::Vec3;

use crate::{camera::Camera, debug_overlay::OverlayVertex};

use super::world::{Block, SubBlockPos, World};

#[derive(Clone, Copy)]
pub struct FaceNormal {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl FaceNormal {
    pub fn as_vec3(self) -> Vec3 {
        Vec3::new(self.x as f32, self.y as f32, self.z as f32)
    }
}

#[derive(Clone, Copy)]
pub struct RaycastHit {
    pub cell: (i64, i32, i64),
    pub previous: (i64, i32, i64),
    pub point: Vec3,
    pub previous_point: Vec3,
    pub normal: FaceNormal,
}

#[derive(Clone, Copy)]
pub struct SubTarget {
    pub base: (i64, i32, i64),
    pub sx: u8,
    pub sy: u8,
    pub sz: u8,
}

pub fn raycast_world_detailed(
    world: &World,
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
    step: f32,
    sub_divisions: Option<u8>,
) -> Option<RaycastHit> {
    let dir = direction.normalize_or_zero();
    if dir.length_squared() <= f32::EPSILON {
        return None;
    }

    if let Some(divisions) = sub_divisions
        && divisions > 1
    {
        let mut distance = step;
        let mut previous_point = origin;
        while distance <= max_distance {
            let point = origin + dir * distance;
            let cell = voxel_coords(point);
            let occupied = is_occupied_sub_cell(world, cell, point, divisions);
            let prev_cell = voxel_coords(previous_point);
            let was_occupied = is_occupied_sub_cell(world, prev_cell, previous_point, divisions);
            if occupied && !was_occupied {
                let normal = if prev_cell != cell {
                    FaceNormal {
                        x: (prev_cell.0 - cell.0).clamp(-1, 1) as i32,
                        y: (prev_cell.1 - cell.1).clamp(-1, 1),
                        z: (prev_cell.2 - cell.2).clamp(-1, 1) as i32,
                    }
                } else {
                    face_normal_from_sub_transition(
                        prev_cell,
                        previous_point,
                        point,
                        divisions,
                        dir,
                    )
                };
                return Some(RaycastHit {
                    cell,
                    previous: prev_cell,
                    point,
                    previous_point,
                    normal,
                });
            }
            previous_point = point;
            distance += step;
        }
        return None;
    }

    let mut previous = voxel_coords(origin);
    let mut previous_point = origin;
    let mut distance = 0.0;
    while distance <= max_distance {
        let point = origin + dir * distance;
        let cell = voxel_coords(point);
        if cell != previous {
            if world.is_solid_i64(cell.0, cell.1, cell.2) {
                let normal = FaceNormal {
                    x: (previous.0 - cell.0).clamp(-1, 1) as i32,
                    y: (previous.1 - cell.1).clamp(-1, 1),
                    z: (previous.2 - cell.2).clamp(-1, 1) as i32,
                };
                return Some(RaycastHit {
                    cell,
                    previous,
                    point,
                    previous_point,
                    normal,
                });
            }
            previous = cell;
        }
        previous_point = point;
        distance += step;
    }

    None
}

pub fn voxel_coords(point: Vec3) -> (i64, i32, i64) {
    (
        point.x.floor() as i64,
        point.y.floor() as i32,
        point.z.floor() as i64,
    )
}

pub fn sub_target_from_world_point(base: (i64, i32, i64), point: Vec3, divisions: u8) -> SubTarget {
    let d = divisions.max(1) as f32;
    let local = point - Vec3::new(base.0 as f32, base.1 as f32, base.2 as f32);
    let to_index = |value: f32| -> u8 { (value.clamp(0.0, 0.9999) * d).floor() as u8 };
    SubTarget {
        base,
        sx: to_index(local.x),
        sy: to_index(local.y),
        sz: to_index(local.z),
    }
}

pub fn find_sub_block_at_point(
    world: &World,
    cell: (i64, i32, i64),
    point: Vec3,
) -> Option<(SubBlockPos, Block)> {
    let candidates = world.sub_blocks_in_cell(cell.0, cell.1, cell.2);
    candidates.into_iter().find(|(sub, block)| {
        if !block.is_solid() {
            return false;
        }
        let (min, max) = sub_pos_bounds(*sub);
        point.x >= min.x
            && point.x < max.x
            && point.y >= min.y
            && point.y < max.y
            && point.z >= min.z
            && point.z < max.z
    })
}

pub fn sub_slot_overlaps_existing(
    world: &World,
    base: (i64, i32, i64),
    divisions: u8,
    sx: u8,
    sy: u8,
    sz: u8,
) -> bool {
    let (new_min, new_max) = sub_target_bounds(base, divisions, sx, sy, sz);
    for (sub, block) in world.sub_blocks_in_cell(base.0, base.1, base.2) {
        if !block.is_solid() {
            continue;
        }
        let (min, max) = sub_pos_bounds(sub);
        if aabb_overlap(new_min, new_max, min, max) {
            return true;
        }
    }
    false
}

pub fn direction_to_screen(
    camera: Camera,
    direction: Vec3,
    width: f32,
    height: f32,
) -> Option<(f32, f32)> {
    // Keep the projected sky marker point comfortably inside the camera far plane.
    let project_distance = (camera.lens.z_far * 0.72).clamp(96.0, 900.0);
    let world_point = camera.position + direction.normalize_or_zero() * project_distance;
    world_to_screen(camera, world_point, width, height)
}

pub fn world_to_screen(camera: Camera, point: Vec3, width: f32, height: f32) -> Option<(f32, f32)> {
    let clip = camera.view_proj() * point.extend(1.0);
    if clip.w <= 0.0 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    if ndc.z < 0.0 || ndc.z > 1.0 {
        return None;
    }
    let x = (ndc.x * 0.5 + 0.5) * width;
    let y = (1.0 - (ndc.y * 0.5 + 0.5)) * height;
    Some((x, y))
}

pub fn face_plane_points(
    cell: (i64, i32, i64),
    normal: FaceNormal,
    divisions: u8,
) -> Vec<(Vec3, Vec3)> {
    let d = divisions.max(1) as f32;
    let mut lines = Vec::with_capacity((divisions as usize + 1) * 2);
    let eps = 0.002_f32;
    let bx = cell.0 as f32;
    let by = cell.1 as f32;
    let bz = cell.2 as f32;
    let normal_vec = normal.as_vec3() * eps;

    for i in 0..=divisions {
        let t = i as f32 / d;
        let (a0, a1, b0, b1) = if normal.x != 0 {
            let fx = bx + if normal.x > 0 { 1.0 } else { 0.0 };
            (
                Vec3::new(fx, by + t, bz),
                Vec3::new(fx, by + t, bz + 1.0),
                Vec3::new(fx, by, bz + t),
                Vec3::new(fx, by + 1.0, bz + t),
            )
        } else if normal.y != 0 {
            let fy = by + if normal.y > 0 { 1.0 } else { 0.0 };
            (
                Vec3::new(bx + t, fy, bz),
                Vec3::new(bx + t, fy, bz + 1.0),
                Vec3::new(bx, fy, bz + t),
                Vec3::new(bx + 1.0, fy, bz + t),
            )
        } else {
            let fz = bz + if normal.z > 0 { 1.0 } else { 0.0 };
            (
                Vec3::new(bx + t, by, fz),
                Vec3::new(bx + t, by + 1.0, fz),
                Vec3::new(bx, by + t, fz),
                Vec3::new(bx + 1.0, by + t, fz),
            )
        };
        lines.push((a0 + normal_vec, a1 + normal_vec));
        lines.push((b0 + normal_vec, b1 + normal_vec));
    }

    lines
}

pub fn sub_block_outline_points(pos: SubBlockPos) -> Vec<(Vec3, Vec3)> {
    let step = 1.0 / pos.divisions.max(1) as f32;
    let min = Vec3::new(
        pos.x as f32 + pos.sx as f32 * step,
        pos.y as f32 + pos.sy as f32 * step,
        pos.z as f32 + pos.sz as f32 * step,
    );
    let max = min + Vec3::splat(step);
    let nudge = 0.0025;
    let c000 = Vec3::new(min.x - nudge, min.y - nudge, min.z - nudge);
    let c001 = Vec3::new(min.x - nudge, min.y - nudge, max.z + nudge);
    let c010 = Vec3::new(min.x - nudge, max.y + nudge, min.z - nudge);
    let c011 = Vec3::new(min.x - nudge, max.y + nudge, max.z + nudge);
    let c100 = Vec3::new(max.x + nudge, min.y - nudge, min.z - nudge);
    let c101 = Vec3::new(max.x + nudge, min.y - nudge, max.z + nudge);
    let c110 = Vec3::new(max.x + nudge, max.y + nudge, min.z - nudge);
    let c111 = Vec3::new(max.x + nudge, max.y + nudge, max.z + nudge);

    vec![
        (c000, c001),
        (c000, c010),
        (c000, c100),
        (c001, c011),
        (c001, c101),
        (c010, c011),
        (c010, c110),
        (c011, c111),
        (c100, c101),
        (c100, c110),
        (c101, c111),
        (c110, c111),
    ]
}

pub fn sub_block_face_plane_points(
    pos: SubBlockPos,
    normal: FaceNormal,
    divisions: u8,
) -> Vec<(Vec3, Vec3)> {
    let grid = divisions.max(1) as usize;
    let step = 1.0 / pos.divisions.max(1) as f32;
    let min = Vec3::new(
        pos.x as f32 + pos.sx as f32 * step,
        pos.y as f32 + pos.sy as f32 * step,
        pos.z as f32 + pos.sz as f32 * step,
    );
    let max = min + Vec3::splat(step);
    let eps = normal.as_vec3() * 0.0025;

    let mut lines = Vec::with_capacity((grid + 1) * 2);
    for i in 0..=grid {
        let t = i as f32 / grid.max(1) as f32;
        let (a0, a1, b0, b1) = if normal.x != 0 {
            let fx = if normal.x > 0 { max.x } else { min.x };
            (
                Vec3::new(fx, min.y + (max.y - min.y) * t, min.z),
                Vec3::new(fx, min.y + (max.y - min.y) * t, max.z),
                Vec3::new(fx, min.y, min.z + (max.z - min.z) * t),
                Vec3::new(fx, max.y, min.z + (max.z - min.z) * t),
            )
        } else if normal.y != 0 {
            let fy = if normal.y > 0 { max.y } else { min.y };
            (
                Vec3::new(min.x + (max.x - min.x) * t, fy, min.z),
                Vec3::new(min.x + (max.x - min.x) * t, fy, max.z),
                Vec3::new(min.x, fy, min.z + (max.z - min.z) * t),
                Vec3::new(max.x, fy, min.z + (max.z - min.z) * t),
            )
        } else {
            let fz = if normal.z > 0 { max.z } else { min.z };
            (
                Vec3::new(min.x + (max.x - min.x) * t, min.y, fz),
                Vec3::new(min.x + (max.x - min.x) * t, max.y, fz),
                Vec3::new(min.x, min.y + (max.y - min.y) * t, fz),
                Vec3::new(max.x, min.y + (max.y - min.y) * t, fz),
            )
        };
        lines.push((a0 + eps, a1 + eps));
        lines.push((b0 + eps, b1 + eps));
    }
    lines
}

pub fn sub_block_face_outline_points(pos: SubBlockPos, normal: FaceNormal) -> Vec<(Vec3, Vec3)> {
    let step = 1.0 / pos.divisions.max(1) as f32;
    let min = Vec3::new(
        pos.x as f32 + pos.sx as f32 * step,
        pos.y as f32 + pos.sy as f32 * step,
        pos.z as f32 + pos.sz as f32 * step,
    );
    let max = min + Vec3::splat(step);
    let eps = normal.as_vec3() * 0.0025;

    let (a, b, c, d) = if normal.x != 0 {
        let fx = if normal.x > 0 { max.x } else { min.x };
        (
            Vec3::new(fx, min.y, min.z),
            Vec3::new(fx, min.y, max.z),
            Vec3::new(fx, max.y, max.z),
            Vec3::new(fx, max.y, min.z),
        )
    } else if normal.y != 0 {
        let fy = if normal.y > 0 { max.y } else { min.y };
        (
            Vec3::new(min.x, fy, min.z),
            Vec3::new(min.x, fy, max.z),
            Vec3::new(max.x, fy, max.z),
            Vec3::new(max.x, fy, min.z),
        )
    } else {
        let fz = if normal.z > 0 { max.z } else { min.z };
        (
            Vec3::new(min.x, min.y, fz),
            Vec3::new(min.x, max.y, fz),
            Vec3::new(max.x, max.y, fz),
            Vec3::new(max.x, min.y, fz),
        )
    };

    vec![
        (a + eps, b + eps),
        (b + eps, c + eps),
        (c + eps, d + eps),
        (d + eps, a + eps),
    ]
}

pub fn push_screen_line(
    vertices: &mut Vec<OverlayVertex>,
    a: (f32, f32),
    b: (f32, f32),
    thickness: f32,
    color: [f32; 4],
) {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len = (dx * dx + dy * dy).sqrt();
    if len <= f32::EPSILON {
        return;
    }
    let nx = -dy / len * (thickness * 0.5);
    let ny = dx / len * (thickness * 0.5);
    let p0 = [a.0 + nx, a.1 + ny];
    let p1 = [b.0 + nx, b.1 + ny];
    let p2 = [b.0 - nx, b.1 - ny];
    let p3 = [a.0 - nx, a.1 - ny];
    vertices.extend_from_slice(&[
        OverlayVertex {
            position: p0,
            color,
        },
        OverlayVertex {
            position: p1,
            color,
        },
        OverlayVertex {
            position: p2,
            color,
        },
        OverlayVertex {
            position: p0,
            color,
        },
        OverlayVertex {
            position: p2,
            color,
        },
        OverlayVertex {
            position: p3,
            color,
        },
    ]);
}

fn is_occupied_sub_cell(world: &World, cell: (i64, i32, i64), point: Vec3, _divisions: u8) -> bool {
    if world.block_at_i64(cell.0, cell.1, cell.2).is_solid() {
        return true;
    }
    find_sub_block_at_point(world, cell, point).is_some()
}

fn face_normal_from_direction(dir: Vec3) -> FaceNormal {
    let ax = dir.x.abs();
    let ay = dir.y.abs();
    let az = dir.z.abs();
    if ax >= ay && ax >= az {
        FaceNormal {
            x: if dir.x > 0.0 { -1 } else { 1 },
            y: 0,
            z: 0,
        }
    } else if ay >= az {
        FaceNormal {
            x: 0,
            y: if dir.y > 0.0 { -1 } else { 1 },
            z: 0,
        }
    } else {
        FaceNormal {
            x: 0,
            y: 0,
            z: if dir.z > 0.0 { -1 } else { 1 },
        }
    }
}

fn sub_pos_bounds(pos: SubBlockPos) -> (Vec3, Vec3) {
    let step = 1.0 / pos.divisions as f32;
    let min = Vec3::new(
        pos.x as f32 + pos.sx as f32 * step,
        pos.y as f32 + pos.sy as f32 * step,
        pos.z as f32 + pos.sz as f32 * step,
    );
    let max = min + Vec3::splat(step);
    (min, max)
}

fn sub_target_bounds(base: (i64, i32, i64), divisions: u8, sx: u8, sy: u8, sz: u8) -> (Vec3, Vec3) {
    let step = 1.0 / divisions as f32;
    let min = Vec3::new(
        base.0 as f32 + sx as f32 * step,
        base.1 as f32 + sy as f32 * step,
        base.2 as f32 + sz as f32 * step,
    );
    let max = min + Vec3::splat(step);
    (min, max)
}

fn aabb_overlap(min_a: Vec3, max_a: Vec3, min_b: Vec3, max_b: Vec3) -> bool {
    min_a.x < max_b.x
        && max_a.x > min_b.x
        && min_a.y < max_b.y
        && max_a.y > min_b.y
        && min_a.z < max_b.z
        && max_a.z > min_b.z
}

fn face_normal_from_sub_transition(
    base: (i64, i32, i64),
    from_point: Vec3,
    to_point: Vec3,
    divisions: u8,
    fallback_dir: Vec3,
) -> FaceNormal {
    let from = sub_target_from_world_point(base, from_point, divisions);
    let to = sub_target_from_world_point(base, to_point, divisions);
    let dx = from.sx as i32 - to.sx as i32;
    let dy = from.sy as i32 - to.sy as i32;
    let dz = from.sz as i32 - to.sz as i32;

    if dx != 0 || dy != 0 || dz != 0 {
        return FaceNormal {
            x: dx.clamp(-1, 1),
            y: dy.clamp(-1, 1),
            z: dz.clamp(-1, 1),
        };
    }
    face_normal_from_direction(fallback_dir)
}
