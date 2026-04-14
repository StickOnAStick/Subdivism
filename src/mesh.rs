use bytemuck::{Pod, Zeroable};
use glam::Vec3;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

impl Vertex {
    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

pub fn push_quad(vertices: &mut Vec<Vertex>, corners: [Vec3; 4], color: [f32; 3]) {
    let [a, b, c, d] = corners;
    vertices.extend_from_slice(&[
        Vertex {
            position: a.to_array(),
            color,
        },
        Vertex {
            position: b.to_array(),
            color,
        },
        Vertex {
            position: c.to_array(),
            color,
        },
        Vertex {
            position: a.to_array(),
            color,
        },
        Vertex {
            position: c.to_array(),
            color,
        },
        Vertex {
            position: d.to_array(),
            color,
        },
    ]);
}
