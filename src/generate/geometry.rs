use num::{self, Num, NumCast};

pub trait Unit: Copy + Num {
    fn unit_radius() -> (Self, Self);
    fn unit_width() -> (Self, Self);
}

macro_rules! unit {
    (integer => $($t:ty),*) => {$(
        impl Unit for $t {
            fn unit_radius() -> (Self, Self) {
                use num::{One, Zero};
                (Self::zero(), Self::one() + Self::one())
            }

            fn unit_width() -> (Self, Self) {
                use num::{One, Zero};
                (Self::zero(), Self::one())
            }
        }
    )*};
    (real => $($t:ty),*) => {$(
        impl Unit for $t {
            fn unit_radius() -> (Self, Self) {
                use num::One;
                (-Self::one(), Self::one())
            }

            fn unit_width() -> (Self, Self) {
                use num::One;
                let half = Self::one() / (Self::one() + Self::one());
                (-half, half)
            }
        }
    )*};
}
unit!(integer => i8, i16, i32, i64, u8, u16, u32, u64);
unit!(real => f32, f64);

pub trait Interpolate<T = Self>: Sized {
    type Output;

    fn lerp(self, other: T, f: f64) -> Self::Output;

    fn midpoint(self, other: T) -> Self::Output {
        self.lerp(other, 0.5)
    }
}

impl<T> Interpolate for (T, T)
where
    T: Copy + Num + NumCast,
{
    type Output = Self;

    fn lerp(self, other: Self, f: f64) -> Self::Output {
        (lerp(self.0, other.0, f), lerp(self.1, other.1, f))
    }
}

impl<T> Interpolate for (T, T, T)
where
    T: Copy + Num + NumCast,
{
    type Output = Self;

    fn lerp(self, other: Self, f: f64) -> Self::Output {
        (
            lerp(self.0, other.0, f),
            lerp(self.1, other.1, f),
            lerp(self.2, other.2, f),
        )
    }
}

fn lerp<T>(a: T, b: T, f: f64) -> T
where
    T: Num + NumCast,
{
    let f = num::clamp(f, 0.0, 1.0);
    let af = <f64 as NumCast>::from(a).unwrap() * (1.0 - f);
    let bf = <f64 as NumCast>::from(b).unwrap() * f;
    <T as NumCast>::from(af + bf).unwrap()
}

#[cfg(feature = "geometry-nalgebra")]
mod feature {
    use nalgebra::{Point2, Point3, Scalar, Vector2, Vector3};

    use super::*;

    impl<T> Interpolate for Point2<T>
    where
        T: NumCast + Scalar + Unit,
    {
        type Output = Self;

        fn lerp(self, other: Self, f: f64) -> Self::Output {
            Point2::new(lerp(self.x, other.x, f), lerp(self.y, other.y, f))
        }
    }

    impl<T> Interpolate for Point3<T>
    where
        T: NumCast + Scalar + Unit,
    {
        type Output = Self;

        fn lerp(self, other: Self, f: f64) -> Self::Output {
            Point3::new(
                lerp(self.x, other.x, f),
                lerp(self.y, other.y, f),
                lerp(self.z, other.z, f),
            )
        }
    }

    impl<T> Interpolate for Vector2<T>
    where
        T: NumCast + Scalar + Unit,
    {
        type Output = Self;

        fn lerp(self, other: Self, f: f64) -> Self::Output {
            Vector2::new(lerp(self.x, other.x, f), lerp(self.y, other.y, f))
        }
    }

    impl<T> Interpolate for Vector3<T>
    where
        T: NumCast + Scalar + Unit,
    {
        type Output = Self;

        fn lerp(self, other: Self, f: f64) -> Self::Output {
            Vector3::new(
                lerp(self.x, other.x, f),
                lerp(self.y, other.y, f),
                lerp(self.z, other.z, f),
            )
        }
    }
}

#[cfg(not(feature = "geometry-nalgebra"))]
mod feature {}

pub use self::feature::*;
