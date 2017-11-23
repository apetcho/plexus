use arrayvec::ArrayVec;
use num::Integer;
use std::iter::FromIterator;
use std::marker::PhantomData;
use std::mem;

use generate::decompose::IntoVertices;

pub trait Topological: Sized {
    type Vertex: Clone;
}

pub trait Polygonal: Topological {}

pub trait Arity {
    const ARITY: usize;
}

pub trait MapVerticesInto<T, U>: Topological<Vertex = T>
where
    T: Clone,
    U: Clone,
{
    type Output: Topological<Vertex = U>;

    fn map_vertices_into<F>(self, f: F) -> Self::Output
    where
        F: FnMut(T) -> U;
}

pub trait MapVertices<T, U>: Sized
where
    T: Clone,
    U: Clone,
{
    fn map_vertices<F>(self, f: F) -> Map<Self, T, U, F>
    where
        F: FnMut(T) -> U;
}

/// Zips the vertices of topologies into a single topology.
///
/// For example, given three `Triangle`s with vertex types of `A`, `B`, and
/// `C`, the result of the zip would be a single `Triangle` with a vertex type
/// of `(A, B, C)`. This is useful for unifying topology streams of different
/// attributes like position, index, and normal into a single stream.
///
/// This trait is implemented for tuples of topological types (`Triangle`,
/// `Polygon`, etc.).
///
/// # Examples
///
/// Zip the vertices of three `Triangle`s:
///
/// ```rust
/// # extern crate nalgebra;
/// # extern crate num;
/// # extern crate plexus;
/// use nalgebra::{Point3, Vector3};
/// use num::Zero;
/// use plexus::generate::Triangle;
/// use plexus::prelude::*;
///
/// # fn main() {
/// let t1 = Triangle::converged(Point3::<f32>::origin());
/// let t2 = Triangle::converged(Vector3::<f32>::zero());
/// let t3 = Triangle::converged(0u32);
/// let tz = (t1, t2, t3).zip_vertices_into();
/// let (point, vector, scalar) = tz.a;
/// # }
/// ```
///
/// Create a topological stream of position and texture coordinate data for a
/// cube using `izip` from the [itertools](https://crates.io/crates/itertools)
/// crate:
///
/// ```rust
/// # extern crate decorum;
/// # #[macro_use]
/// # extern crate itertools;
/// # extern crate num;
/// # extern crate plexus;
/// use plexus::generate::cube::Cube;
/// use plexus::prelude::*;
///
/// # use decorum::R32;
/// # use num::One;
/// # fn map_to_color(texture: &Duplet<R32>) -> Triplet<R32> {
/// #     Triplet(One::one(), One::one(), One::one())
/// # }
///
/// # fn main() {
/// let cube = Cube::new();
/// let polygons = izip!(
///     cube.polygons_with_position(),
///     cube.polygons_with_texture()
/// ).map(|polygons| polygons.zip_vertices_into())
///     .map_vertices(|(position, texture)| (position, texture, map_to_color(&texture)))
///     .triangulate()
///     .collect::<Vec<_>>();
/// # }
/// ```
pub trait ZipVerticesInto {
    type Output: FromIterator<<Self::Output as Topological>::Vertex> + Topological;

    fn zip_vertices_into(self) -> Self::Output;
}

// TODO: Using `FromIterator` to implement this is fragile. This is especially
//       true for `Polygon`, because the arity of the polygons that are zipped
//       may not be the same. This could cause panics or unexpected behavior.
//       It may be a better idea to hide `ZipVerticesInto` behind a more
//       restricted interface.
macro_rules! zip_vertices_into {
    (topology => $t:ident, geometries => ($($g:ident),*)) => (
        #[allow(non_snake_case)]
        impl<$($g: Clone),*> ZipVerticesInto for ($($t<$g>),*) {
            type Output = $t<($($g),*)>;

            fn zip_vertices_into(self) -> Self::Output {
                let ($($g,)*) = self;
                izip!($($g.into_vertices()),*).collect()
            }
        }
    );
}

pub trait Rotate {
    fn rotate(&mut self, n: isize);
}

impl<I, T, U, P, Q> MapVertices<T, U> for I
where
    I: Iterator<Item = P>,
    P: MapVerticesInto<T, U, Output = Q>,
    Q: Topological<Vertex = U>,
    T: Clone,
    U: Clone,
{
    fn map_vertices<F>(self, f: F) -> Map<Self, T, U, F>
    where
        F: FnMut(T) -> U,
    {
        Map::new(self, f)
    }
}

pub struct Map<I, T, U, F> {
    input: I,
    f: F,
    phantom: PhantomData<(T, U)>,
}

impl<I, T, U, F> Map<I, T, U, F> {
    fn new(input: I, f: F) -> Self {
        Map {
            input: input,
            f: f,
            phantom: PhantomData,
        }
    }
}

impl<I, T, U, F, P, Q> Iterator for Map<I, T, U, F>
where
    I: Iterator<Item = P>,
    F: FnMut(T) -> U,
    P: MapVerticesInto<T, U, Output = Q>,
    Q: Topological<Vertex = U>,
    T: Clone,
    U: Clone,
{
    type Item = Q;

    fn next(&mut self) -> Option<Self::Item> {
        self.input
            .next()
            .map(|topology| topology.map_vertices_into(&mut self.f))
    }
}

pub struct Line<T> {
    pub a: T,
    pub b: T,
}

impl<T> Line<T> {
    pub fn new(a: T, b: T) -> Self {
        Line { a, b }
    }

    pub fn converged(value: T) -> Self
    where
        T: Clone,
    {
        Line::new(value.clone(), value)
    }
}

impl<T> Arity for Line<T> {
    const ARITY: usize = 2;
}

impl<T> FromIterator<T> for Line<T> {
    fn from_iter<I>(input: I) -> Self
    where
        I: IntoIterator<Item = T>,
    {
        let mut input = input.into_iter();
        Line::new(input.next().unwrap(), input.next().unwrap())
    }
}
zip_vertices_into!(topology => Line, geometries => (A, B));
zip_vertices_into!(topology => Line, geometries => (A, B, C));
zip_vertices_into!(topology => Line, geometries => (A, B, C, D));

impl<T, U> MapVerticesInto<T, U> for Line<T>
where
    T: Clone,
    U: Clone,
{
    type Output = Line<U>;

    fn map_vertices_into<F>(self, mut f: F) -> Self::Output
    where
        F: FnMut(T) -> U,
    {
        let Line { a, b } = self;
        Line::new(f(a), f(b))
    }
}

impl<T> Topological for Line<T>
where
    T: Clone,
{
    type Vertex = T;
}

impl<T> Rotate for Line<T>
where
    T: Clone,
{
    fn rotate(&mut self, n: isize) {
        if n % 2 != 0 {
            mem::swap(&mut self.a, &mut self.b);
        }
    }
}

pub struct Triangle<T> {
    pub a: T,
    pub b: T,
    pub c: T,
}

impl<T> Triangle<T> {
    pub fn new(a: T, b: T, c: T) -> Self {
        Triangle { a, b, c }
    }

    pub fn converged(value: T) -> Self
    where
        T: Clone,
    {
        Triangle::new(value.clone(), value.clone(), value)
    }
}

impl<T> Arity for Triangle<T> {
    const ARITY: usize = 3;
}

impl<T> FromIterator<T> for Triangle<T> {
    fn from_iter<I>(input: I) -> Self
    where
        I: IntoIterator<Item = T>,
    {
        let mut input = input.into_iter();
        Triangle::new(
            input.next().unwrap(),
            input.next().unwrap(),
            input.next().unwrap(),
        )
    }
}
zip_vertices_into!(topology => Triangle, geometries => (A, B));
zip_vertices_into!(topology => Triangle, geometries => (A, B, C));
zip_vertices_into!(topology => Triangle, geometries => (A, B, C, D));

impl<T, U> MapVerticesInto<T, U> for Triangle<T>
where
    T: Clone,
    U: Clone,
{
    type Output = Triangle<U>;

    fn map_vertices_into<F>(self, mut f: F) -> Self::Output
    where
        F: FnMut(T) -> U,
    {
        let Triangle { a, b, c } = self;
        Triangle::new(f(a), f(b), f(c))
    }
}

impl<T> Topological for Triangle<T>
where
    T: Clone,
{
    type Vertex = T;
}

impl<T> Polygonal for Triangle<T>
where
    T: Clone,
{
}

impl<T> Rotate for Triangle<T>
where
    T: Clone,
{
    fn rotate(&mut self, n: isize) {
        let n = umod(n, Self::ARITY as isize);
        if n == 1 {
            mem::swap(&mut self.a, &mut self.b);
            mem::swap(&mut self.b, &mut self.c);
        }
        else if n == 2 {
            mem::swap(&mut self.c, &mut self.b);
            mem::swap(&mut self.b, &mut self.a);
        }
    }
}

pub struct Quad<T> {
    pub a: T,
    pub b: T,
    pub c: T,
    pub d: T,
}

impl<T> Quad<T> {
    pub fn new(a: T, b: T, c: T, d: T) -> Self {
        Quad { a, b, c, d }
    }

    pub fn converged(value: T) -> Self
    where
        T: Clone,
    {
        Quad::new(value.clone(), value.clone(), value.clone(), value)
    }
}

impl<T> Arity for Quad<T> {
    const ARITY: usize = 4;
}

impl<T> FromIterator<T> for Quad<T> {
    fn from_iter<I>(input: I) -> Self
    where
        I: IntoIterator<Item = T>,
    {
        let mut input = input.into_iter();
        Quad::new(
            input.next().unwrap(),
            input.next().unwrap(),
            input.next().unwrap(),
            input.next().unwrap(),
        )
    }
}
zip_vertices_into!(topology => Quad, geometries => (A, B));
zip_vertices_into!(topology => Quad, geometries => (A, B, C));
zip_vertices_into!(topology => Quad, geometries => (A, B, C, D));

impl<T, U> MapVerticesInto<T, U> for Quad<T>
where
    T: Clone,
    U: Clone,
{
    type Output = Quad<U>;

    fn map_vertices_into<F>(self, mut f: F) -> Self::Output
    where
        F: FnMut(T) -> U,
    {
        let Quad { a, b, c, d } = self;
        Quad::new(f(a), f(b), f(c), f(d))
    }
}

impl<T> Topological for Quad<T>
where
    T: Clone,
{
    type Vertex = T;
}

impl<T> Polygonal for Quad<T>
where
    T: Clone,
{
}

impl<T> Rotate for Quad<T>
where
    T: Clone,
{
    fn rotate(&mut self, n: isize) {
        let n = umod(n, Self::ARITY as isize);
        if n == 1 {
            mem::swap(&mut self.a, &mut self.b);
            mem::swap(&mut self.b, &mut self.c);
            mem::swap(&mut self.c, &mut self.d);
        }
        else if n == 2 {
            mem::swap(&mut self.a, &mut self.c);
            mem::swap(&mut self.b, &mut self.d);
        }
        else if n == 3 {
            mem::swap(&mut self.d, &mut self.c);
            mem::swap(&mut self.c, &mut self.b);
            mem::swap(&mut self.b, &mut self.a);
        }
    }
}

pub enum Polygon<T> {
    Triangle(Triangle<T>),
    Quad(Quad<T>),
}

impl<T> From<Triangle<T>> for Polygon<T> {
    fn from(triangle: Triangle<T>) -> Self {
        Polygon::Triangle(triangle)
    }
}

impl<T> From<Quad<T>> for Polygon<T> {
    fn from(quad: Quad<T>) -> Self {
        Polygon::Quad(quad)
    }
}

impl<T> FromIterator<T> for Polygon<T> {
    fn from_iter<I>(input: I) -> Self
    where
        I: IntoIterator<Item = T>,
    {
        // Associated constants cannot be used in constant expressions, so the
        // size of the `ArrayVec` uses a literal instead of `Quad::<T>::ARITY`.
        let input = input
            .into_iter()
            .take(Quad::<T>::ARITY)
            .collect::<ArrayVec<[T; 4]>>();
        match input.len() {
            Quad::<T>::ARITY => Polygon::Quad(Quad::from_iter(input)),
            Triangle::<T>::ARITY => Polygon::Triangle(Triangle::from_iter(input)),
            _ => panic!(),
        }
    }
}
zip_vertices_into!(topology => Polygon, geometries => (A, B));
zip_vertices_into!(topology => Polygon, geometries => (A, B, C));
zip_vertices_into!(topology => Polygon, geometries => (A, B, C, D));

impl<T, U> MapVerticesInto<T, U> for Polygon<T>
where
    T: Clone,
    U: Clone,
{
    type Output = Polygon<U>;

    fn map_vertices_into<F>(self, f: F) -> Self::Output
    where
        F: FnMut(T) -> U,
    {
        match self {
            Polygon::Triangle(triangle) => Polygon::Triangle(triangle.map_vertices_into(f)),
            Polygon::Quad(quad) => Polygon::Quad(quad.map_vertices_into(f)),
        }
    }
}

impl<T> Topological for Polygon<T>
where
    T: Clone,
{
    type Vertex = T;
}

impl<T> Polygonal for Polygon<T>
where
    T: Clone,
{
}

impl<T> Rotate for Polygon<T>
where
    T: Clone,
{
    fn rotate(&mut self, n: isize) {
        match *self {
            Polygon::Triangle(ref mut triangle) => triangle.rotate(n),
            Polygon::Quad(ref mut quad) => quad.rotate(n),
        }
    }
}

fn umod<T>(n: T, m: T) -> T
where
    T: Copy + Integer,
{
    ((n % m) + m) % m
}
