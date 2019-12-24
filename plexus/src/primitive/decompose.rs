//! Topological decomposition and tessellation.
//!
//! The `Decompose` iterator uses various traits to decompose and tessellate
//! streams of topological structures.

use arrayvec::ArrayVec;
use std::collections::VecDeque;
use std::iter::IntoIterator;
use theon::ops::Interpolate;
use theon::IntoItems;

use crate::primitive::{Edge, Polygon, Polygonal, Tetragon, Topological, Trigon};

pub struct Decompose<I, P, Q, R>
where
    R: IntoIterator<Item = Q>,
{
    input: I,
    output: VecDeque<Q>,
    f: fn(P) -> R,
}

impl<I, P, Q, R> Decompose<I, P, Q, R>
where
    R: IntoIterator<Item = Q>,
{
    pub(in crate::primitive) fn new(input: I, f: fn(P) -> R) -> Self {
        Decompose {
            input,
            output: VecDeque::new(),
            f,
        }
    }
}

impl<I, P, R> Decompose<I, P, P, R>
where
    I: Iterator<Item = P>,
    R: IntoIterator<Item = P>,
{
    /// Reapplies a congruent decomposition.
    ///
    /// A decomposition is _congruent_ if its input and output types are the
    /// same. This is useful when the number of applications is somewhat large
    /// or variable, in which case chaining calls is impractical or impossible.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate decorum;
    /// # extern crate nalgebra;
    /// # extern crate plexus;
    /// #
    /// use decorum::N64;
    /// use nalgebra::Point3;
    /// use plexus::index::{Flat4, HashIndexer};
    /// use plexus::prelude::*;
    /// use plexus::primitive::cube::Cube;
    /// use plexus::primitive::generate::Position;
    ///
    /// let (indices, positions) = Cube::new()
    ///     .polygons::<Position<Point3<N64>>>()
    ///     .subdivide()
    ///     .remap(7) // 8 subdivision operations are applied.
    ///     .index_vertices::<Flat4, _>(HashIndexer::default());
    /// ```
    pub fn remap(self, n: usize) -> Decompose<impl Iterator<Item = P>, P, P, R> {
        let Decompose { input, output, f } = self;
        Decompose::new(output.into_iter().rev().chain(remap(n, input, f)), f)
    }
}

impl<I, P, Q, R> Iterator for Decompose<I, P, Q, R>
where
    I: Iterator<Item = P>,
    R: IntoIterator<Item = Q>,
{
    type Item = Q;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(ngon) = self.output.pop_front() {
                return Some(ngon);
            }
            if let Some(ngon) = self.input.next() {
                self.output.extend((self.f)(ngon));
            }
            else {
                return None;
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (lower, _) = self.input.size_hint();
        (lower, None)
    }
}

pub trait IntoVertices: Topological {
    type Output: IntoIterator<Item = Self::Vertex>;

    fn into_vertices(self) -> Self::Output;
}

impl<T> IntoVertices for T
where
    T: IntoItems + Topological,
{
    type Output = <T as IntoItems>::Output;

    fn into_vertices(self) -> Self::Output {
        self.into_items()
    }
}

pub trait IntoEdges: Topological {
    type Output: IntoIterator<Item = Edge<Self::Vertex>>;

    fn into_edges(self) -> Self::Output;
}

pub trait IntoTrigons: Polygonal {
    type Output: IntoIterator<Item = Trigon<Self::Vertex>>;

    fn into_trigons(self) -> Self::Output;
}

pub trait IntoSubdivisions: Polygonal {
    type Output: IntoIterator<Item = Self>;

    fn into_subdivisions(self) -> Self::Output;
}

pub trait IntoTetrahedrons: Polygonal {
    fn into_tetrahedrons(self) -> ArrayVec<[Trigon<Self::Vertex>; 4]>;
}

impl<T> IntoEdges for Edge<T> {
    type Output = ArrayVec<[Edge<Self::Vertex>; 1]>;

    fn into_edges(self) -> Self::Output {
        ArrayVec::from([self])
    }
}

impl<T> IntoEdges for Trigon<T>
where
    T: Clone,
{
    type Output = ArrayVec<[Edge<Self::Vertex>; 3]>;

    fn into_edges(self) -> Self::Output {
        let [a, b, c] = self.into_array();
        ArrayVec::from([
            Edge::new(a.clone(), b.clone()),
            Edge::new(b, c.clone()),
            Edge::new(c, a),
        ])
    }
}

impl<T> IntoEdges for Tetragon<T>
where
    T: Clone,
{
    type Output = ArrayVec<[Edge<Self::Vertex>; 4]>;

    fn into_edges(self) -> Self::Output {
        let [a, b, c, d] = self.into_array();
        ArrayVec::from([
            Edge::new(a.clone(), b.clone()),
            Edge::new(b, c.clone()),
            Edge::new(c, d.clone()),
            Edge::new(d, a),
        ])
    }
}

impl<T> IntoEdges for Polygon<T>
where
    T: Clone,
{
    type Output = Vec<Edge<Self::Vertex>>;

    fn into_edges(self) -> Self::Output {
        match self {
            Polygon::N3(trigon) => trigon.into_edges().into_iter().collect(),
            Polygon::N4(tetragon) => tetragon.into_edges().into_iter().collect(),
        }
    }
}

impl<T> IntoTrigons for Trigon<T> {
    type Output = ArrayVec<[Trigon<Self::Vertex>; 1]>;

    fn into_trigons(self) -> Self::Output {
        ArrayVec::from([self])
    }
}

impl<T> IntoTrigons for Tetragon<T>
where
    T: Clone,
{
    type Output = ArrayVec<[Trigon<Self::Vertex>; 2]>;

    fn into_trigons(self) -> Self::Output {
        let [a, b, c, d] = self.into_array();
        ArrayVec::from([Trigon::new(a.clone(), b, c.clone()), Trigon::new(c, d, a)])
    }
}

impl<T> IntoTrigons for Polygon<T>
where
    T: Clone,
{
    type Output = Vec<Trigon<Self::Vertex>>;

    fn into_trigons(self) -> Self::Output {
        match self {
            Polygon::N3(trigon) => trigon.into_trigons().into_iter().collect(),
            Polygon::N4(tetragon) => tetragon.into_trigons().into_iter().collect(),
        }
    }
}

impl<T> IntoSubdivisions for Trigon<T>
where
    T: Clone + Interpolate<Output = T>,
{
    type Output = ArrayVec<[Trigon<Self::Vertex>; 2]>;

    fn into_subdivisions(self) -> Self::Output {
        let [a, b, c] = self.into_array();
        let ac = a.clone().midpoint(c.clone());
        ArrayVec::from([Trigon::new(b.clone(), ac.clone(), a), Trigon::new(c, ac, b)])
    }
}

impl<T> IntoSubdivisions for Tetragon<T>
where
    T: Clone + Interpolate<Output = T>,
{
    type Output = ArrayVec<[Tetragon<Self::Vertex>; 4]>;

    fn into_subdivisions(self) -> Self::Output {
        let [a, b, c, d] = self.into_array();
        let ab = a.clone().midpoint(b.clone());
        let bc = b.clone().midpoint(c.clone());
        let cd = c.clone().midpoint(d.clone());
        let da = d.clone().midpoint(a.clone());
        let ac = a.clone().midpoint(c.clone()); // Diagonal.
        ArrayVec::from([
            Tetragon::new(a, ab.clone(), ac.clone(), da.clone()),
            Tetragon::new(ab, b, bc.clone(), ac.clone()),
            Tetragon::new(ac.clone(), bc, c, cd.clone()),
            Tetragon::new(da, ac, cd, d),
        ])
    }
}

impl<T> IntoTetrahedrons for Tetragon<T>
where
    T: Clone + Interpolate<Output = T>,
{
    fn into_tetrahedrons(self) -> ArrayVec<[Trigon<Self::Vertex>; 4]> {
        let [a, b, c, d] = self.into_array();
        let ac = a.clone().midpoint(c.clone()); // Diagonal.
        ArrayVec::from([
            Trigon::new(a.clone(), b.clone(), ac.clone()),
            Trigon::new(b, c.clone(), ac.clone()),
            Trigon::new(c, d.clone(), ac.clone()),
            Trigon::new(d, a, ac),
        ])
    }
}

impl<T> IntoSubdivisions for Polygon<T>
where
    T: Clone + Interpolate<Output = T>,
{
    type Output = Vec<Self>;

    fn into_subdivisions(self) -> Self::Output {
        match self {
            Polygon::N3(trigon) => trigon
                .into_subdivisions()
                .into_iter()
                .map(|trigon| trigon.into())
                .collect(),
            Polygon::N4(tetragon) => tetragon
                .into_subdivisions()
                .into_iter()
                .map(|tetragon| tetragon.into())
                .collect(),
        }
    }
}

pub trait Vertices<P>: Sized
where
    P: IntoVertices,
{
    fn vertices(self) -> Decompose<Self, P, P::Vertex, P::Output>;
}

impl<I, P> Vertices<P> for I
where
    I: Iterator<Item = P>,
    P: IntoVertices,
{
    fn vertices(self) -> Decompose<Self, P, P::Vertex, P::Output> {
        Decompose::new(self, P::into_vertices)
    }
}

pub trait Edges<P>: Sized
where
    P: IntoEdges,
{
    fn edges(self) -> Decompose<Self, P, Edge<P::Vertex>, P::Output>;
}

impl<I, P> Edges<P> for I
where
    I: Iterator<Item = P>,
    P: IntoEdges,
    P::Vertex: Clone,
{
    fn edges(self) -> Decompose<Self, P, Edge<P::Vertex>, P::Output> {
        Decompose::new(self, P::into_edges)
    }
}

pub trait Triangulate<P>: Sized
where
    P: IntoTrigons,
{
    fn triangulate(self) -> Decompose<Self, P, Trigon<P::Vertex>, P::Output>;
}

impl<I, P> Triangulate<P> for I
where
    I: Iterator<Item = P>,
    P: IntoTrigons,
{
    fn triangulate(self) -> Decompose<Self, P, Trigon<P::Vertex>, P::Output> {
        Decompose::new(self, P::into_trigons)
    }
}

pub trait Subdivide<P>: Sized
where
    P: IntoSubdivisions,
{
    fn subdivide(self) -> Decompose<Self, P, P, P::Output>;
}

impl<I, P> Subdivide<P> for I
where
    I: Iterator<Item = P>,
    P: IntoSubdivisions,
{
    fn subdivide(self) -> Decompose<Self, P, P, P::Output> {
        Decompose::new(self, P::into_subdivisions)
    }
}

pub trait Tetrahedrons<T>: Sized {
    #[allow(clippy::type_complexity)]
    fn tetrahedrons(self) -> Decompose<Self, Tetragon<T>, Trigon<T>, ArrayVec<[Trigon<T>; 4]>>;
}

impl<I, T> Tetrahedrons<T> for I
where
    I: Iterator<Item = Tetragon<T>>,
    T: Clone + Interpolate<Output = T>,
{
    #[allow(clippy::type_complexity)]
    fn tetrahedrons(self) -> Decompose<Self, Tetragon<T>, Trigon<T>, ArrayVec<[Trigon<T>; 4]>> {
        Decompose::new(self, Tetragon::into_tetrahedrons)
    }
}

fn remap<I, P, R, F>(n: usize, ngons: I, f: F) -> Vec<P>
where
    I: IntoIterator<Item = P>,
    R: IntoIterator<Item = P>,
    F: Fn(P) -> R,
{
    let mut ngons: Vec<_> = ngons.into_iter().collect();
    for _ in 0..n {
        ngons = ngons.into_iter().flat_map(&f).collect();
    }
    ngons
}
