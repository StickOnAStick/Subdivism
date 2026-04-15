#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrefabKind {
    Tree,
    House,
    Dungeon,
    Cave,
    Mineshaft,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpawnBounds {
    Circle {
        center_x: i64,
        center_z: i64,
        radius_chunks: i64,
    },
    Rect {
        min_chunk_x: i64,
        max_chunk_x: i64,
        min_chunk_z: i64,
        max_chunk_z: i64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainMutation {
    pub flatten_radius_blocks: f32,
    pub carve_radius_blocks: f32,
    pub carve_depth_blocks: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PrefabArchetype {
    pub id: String,
    pub kind: PrefabKind,
    pub footprint_chunks: (i64, i64),
    pub spawn_chance_per_chunk: f32,
    pub allowed_biomes: Vec<String>,
    pub bounds: Option<SpawnBounds>,
    pub terrain_mutation: Option<TerrainMutation>,
}

#[derive(Clone, Debug, Default)]
pub struct PrefabCatalog {
    pub archetypes: Vec<PrefabArchetype>,
}

impl PrefabCatalog {
    pub fn with_defaults() -> Self {
        Self {
            archetypes: vec![
                PrefabArchetype {
                    id: "oak_tree".to_string(),
                    kind: PrefabKind::Tree,
                    footprint_chunks: (1, 1),
                    spawn_chance_per_chunk: 0.28,
                    allowed_biomes: vec!["meadow".to_string(), "valley".to_string()],
                    bounds: None,
                    terrain_mutation: Some(TerrainMutation {
                        flatten_radius_blocks: 2.0,
                        carve_radius_blocks: 0.0,
                        carve_depth_blocks: 0.0,
                    }),
                },
                PrefabArchetype {
                    id: "village_house".to_string(),
                    kind: PrefabKind::House,
                    footprint_chunks: (1, 1),
                    spawn_chance_per_chunk: 0.04,
                    allowed_biomes: vec!["meadow".to_string(), "desert".to_string()],
                    bounds: Some(SpawnBounds::Circle {
                        center_x: 0,
                        center_z: 0,
                        radius_chunks: 96,
                    }),
                    terrain_mutation: Some(TerrainMutation {
                        flatten_radius_blocks: 8.0,
                        carve_radius_blocks: 0.0,
                        carve_depth_blocks: 0.0,
                    }),
                },
                PrefabArchetype {
                    id: "dungeon_room".to_string(),
                    kind: PrefabKind::Dungeon,
                    footprint_chunks: (2, 2),
                    spawn_chance_per_chunk: 0.01,
                    allowed_biomes: vec!["mountain".to_string(), "valley".to_string()],
                    bounds: Some(SpawnBounds::Rect {
                        min_chunk_x: -256,
                        max_chunk_x: 256,
                        min_chunk_z: -256,
                        max_chunk_z: 256,
                    }),
                    terrain_mutation: Some(TerrainMutation {
                        flatten_radius_blocks: 0.0,
                        carve_radius_blocks: 14.0,
                        carve_depth_blocks: 9.0,
                    }),
                },
                PrefabArchetype {
                    id: "cave_entrance".to_string(),
                    kind: PrefabKind::Cave,
                    footprint_chunks: (1, 1),
                    spawn_chance_per_chunk: 0.03,
                    allowed_biomes: vec!["mountain".to_string(), "desert".to_string()],
                    bounds: None,
                    terrain_mutation: Some(TerrainMutation {
                        flatten_radius_blocks: 0.0,
                        carve_radius_blocks: 10.0,
                        carve_depth_blocks: 7.0,
                    }),
                },
                PrefabArchetype {
                    id: "mineshaft_segment".to_string(),
                    kind: PrefabKind::Mineshaft,
                    footprint_chunks: (2, 1),
                    spawn_chance_per_chunk: 0.015,
                    allowed_biomes: vec!["mountain".to_string(), "valley".to_string()],
                    bounds: Some(SpawnBounds::Rect {
                        min_chunk_x: -512,
                        max_chunk_x: 512,
                        min_chunk_z: -512,
                        max_chunk_z: 512,
                    }),
                    terrain_mutation: Some(TerrainMutation {
                        flatten_radius_blocks: 0.0,
                        carve_radius_blocks: 8.0,
                        carve_depth_blocks: 6.0,
                    }),
                },
            ],
        }
    }

    pub fn candidates_for_chunk(
        &self,
        chunk_x: i64,
        chunk_z: i64,
        biome: &str,
        seed: i64,
    ) -> Vec<&PrefabArchetype> {
        let mut out = Vec::new();
        for archetype in &self.archetypes {
            if !archetype
                .allowed_biomes
                .iter()
                .any(|allowed| allowed == biome)
            {
                continue;
            }
            if let Some(bounds) = archetype.bounds
                && !chunk_in_bounds(chunk_x, chunk_z, bounds)
            {
                continue;
            }
            let roll = deterministic_roll(seed, chunk_x, chunk_z, &archetype.id);
            if roll <= archetype.spawn_chance_per_chunk.clamp(0.0, 1.0) {
                out.push(archetype);
            }
        }
        out
    }
}

fn chunk_in_bounds(chunk_x: i64, chunk_z: i64, bounds: SpawnBounds) -> bool {
    match bounds {
        SpawnBounds::Circle {
            center_x,
            center_z,
            radius_chunks,
        } => {
            let dx = chunk_x - center_x;
            let dz = chunk_z - center_z;
            dx * dx + dz * dz <= radius_chunks * radius_chunks
        }
        SpawnBounds::Rect {
            min_chunk_x,
            max_chunk_x,
            min_chunk_z,
            max_chunk_z,
        } => {
            (min_chunk_x..=max_chunk_x).contains(&chunk_x)
                && (min_chunk_z..=max_chunk_z).contains(&chunk_z)
        }
    }
}

fn deterministic_roll(seed: i64, chunk_x: i64, chunk_z: i64, id: &str) -> f32 {
    let mut h = seed as u64;
    h ^= (chunk_x as u64).wrapping_mul(0x9E37_79B1_85EB_CA87);
    h ^= (chunk_z as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    for b in id.as_bytes() {
        h ^= (*b as u64).wrapping_mul(0x1000_0000_01B3);
        h = h.rotate_left(13).wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    }
    h ^= h >> 33;
    h = h.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    h ^= h >> 33;
    (h as f64 / u64::MAX as f64) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_catalog_has_structures_and_mutations() {
        let catalog = PrefabCatalog::with_defaults();
        assert!(
            catalog
                .archetypes
                .iter()
                .any(|a| a.kind == PrefabKind::Tree)
        );
        assert!(
            catalog
                .archetypes
                .iter()
                .any(|a| a.kind == PrefabKind::Dungeon)
        );
        assert!(
            catalog
                .archetypes
                .iter()
                .any(|a| a.terrain_mutation.is_some())
        );
    }

    #[test]
    fn bounded_candidates_respect_region() {
        let catalog = PrefabCatalog {
            archetypes: vec![PrefabArchetype {
                id: "bounded_test".to_string(),
                kind: PrefabKind::House,
                footprint_chunks: (1, 1),
                spawn_chance_per_chunk: 1.0,
                allowed_biomes: vec!["meadow".to_string()],
                bounds: Some(SpawnBounds::Circle {
                    center_x: 0,
                    center_z: 0,
                    radius_chunks: 8,
                }),
                terrain_mutation: None,
            }],
        };
        let inside = catalog.candidates_for_chunk(2, -3, "meadow", 42);
        let outside = catalog.candidates_for_chunk(64, 64, "meadow", 42);
        assert_eq!(inside.len(), 1);
        assert!(outside.is_empty());
    }
}
