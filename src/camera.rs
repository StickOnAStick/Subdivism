use glam::{Mat4, Vec3};

/// Camera parameters that are shared by both the player view and free camera.
#[derive(Clone, Copy)]
pub struct CameraLens {
    pub aspect: f32,
    pub fov_y_radians: f32,
    pub z_near: f32,
    pub z_far: f32,
}

impl CameraLens {
    pub fn with_aspect(self, aspect: f32) -> Self {
        Self { aspect, ..self }
    }
}

impl Default for CameraLens {
    fn default() -> Self {
        Self {
            aspect: 16.0 / 9.0,
            fov_y_radians: 45.0_f32.to_radians(),
            z_near: 0.1,
            z_far: 500.0,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Camera {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub lens: CameraLens,
}

impl Camera {
    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        )
        .normalize()
    }

    pub fn view_proj(&self) -> Mat4 {
        let view = Mat4::look_at_rh(self.position, self.position + self.forward(), Vec3::Y);
        let proj = Mat4::perspective_rh(
            self.lens.fov_y_radians,
            self.lens.aspect,
            self.lens.z_near,
            self.lens.z_far,
        );

        // Convert OpenGL-style clip space to wgpu/WebGPU clip space.
        let opengl_to_wgpu = Mat4::from_cols_array(&[
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5, 1.0,
        ]);

        opengl_to_wgpu * proj * view
    }
}
