use std::{fmt, ops::{Add, Mul, Sub}};

#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
pub struct Vector {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl fmt::Display for Vector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(vector : ({}, {}, {}))", self.x, self.y, self.z)
    }
}

/// Overload for the + operator (Vector + Vector).
impl Add for Vector {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
            z: self.z + other.z,
        }
    }
}

/// Overload for the - operator (Vector - Vector).
impl Sub for Vector {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
        }
    }
}

/// Overload for the * operator (Vector * f32 scale).
impl Mul<f32> for Vector {
    type Output = Self;

    fn mul(self, scale: f32) -> Self {
        Self {
            x: self.x * scale,
            y: self.y * scale,
            z: self.z * scale,
        }
    }
}

impl Vector {
    /// Creates a new vector with the specified Cartesian coordinates.
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Returns the distance from the origin.
    pub fn length(&self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    /// Returns the distance from the origin, ignoring the Z axis.
    pub fn length_2d(&self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    /// Returns the distance from the origin, but squared.
    /// This is faster to compute since a square root isn't required.
    pub fn length_sqr(&self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    /// Returns the distance from the origin, ignoring the Z axis and squared.
    pub fn length_2d_sqr(&self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    /// Returns the distance between this vector and another.
    pub fn distance(&self, other: &Vector) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    /// Returns the vector cross product (this x other).
    pub fn cross(&self, other: &Vector) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }

    /// Returns the vector dot product (this . other).
    pub fn dot(&self, other: &Vector) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    /// Modifies the vector to have a length of 1, and returns its original length.
    pub fn norm(&mut self) -> f32 {
        let len = self.length();
        if len != 0.0 {
            self.x /= len;
            self.y /= len;
            self.z /= len;
        }
        len
    }

    /// Returns a string in the form "X Y Z" (Equivalent to ToKVString).
    pub fn to_kv_string(&self) -> String {
        format!("{} {} {}", self.x, self.y, self.z)
    }
}

#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
pub struct QAngle {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl QAngle {
    /// Creates a new QAngle with the specified pitch, yaw, and roll.
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Converts the QAngle (Pitch and Yaw) into a forward directional Vector.
    /// This is the equivalent of the Source Engine's `AngleVectors` function
    pub fn to_forward_vector(&self) -> Vector {
        let pitch_rad = self.x.to_radians();
        let yaw_rad = self.y.to_radians();

        let (sin_pitch, cos_pitch) = pitch_rad.sin_cos();
        let (sin_yaw, cos_yaw) = yaw_rad.sin_cos();

        Vector {
            x: cos_pitch * cos_yaw,
            y: cos_pitch * sin_yaw,
            z: -sin_pitch, // iirc, in Source Engine, positive pitch looks down. Right?
        }
    }
}

impl fmt::Display for QAngle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(qangle : ({}, {}, {}))", self.x, self.y, self.z)
    }
}

impl From<QAngle> for Vector {
    fn from(angle: QAngle) -> Self {
        Self {
            x: angle.x,
            y: angle.y,
            z: angle.z,
        }
    }
}
impl From<Vector> for QAngle {
    fn from(vec: Vector) -> Self {
        Self {
            x: vec.x,
            y: vec.y,
            z: vec.z,
        }
    }
}

#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
pub struct VMatrix {
    pub m: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BBoxT {
    pub mins: Vector,
    pub maxs: Vector,
}

#[repr(C)] pub struct VPlane { _private: [u8; 0] }
