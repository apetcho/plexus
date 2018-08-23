pub mod edge;
pub mod face;
mod region;
pub mod vertex;

use failure::Error;
use std::fmt::Debug;
use std::mem;
use std::ops::{Deref, DerefMut};

use geometry::Geometry;
use graph::container::{Container, Indeterminate};
use graph::mesh::Mesh;
use graph::mutation::face::FaceMutation;
use graph::storage::convert::AsStorage;
use graph::storage::Storage;
use graph::topology::{Edge, Face, Vertex};

pub trait Mutate: Sized {
    type Mutant;
    type Error: Debug;

    fn mutate(mutant: Self::Mutant) -> Self;

    fn commit(self) -> Result<Self::Mutant, Self::Error>;

    fn commit_with<F, T, E>(mut self, f: F) -> Result<(Self::Mutant, T), Self::Error>
    where
        F: FnOnce(&mut Self) -> Result<T, E>,
        E: Into<Self::Error>,
    {
        let output = f(&mut self);
        match output {
            Ok(value) => self.commit().map(|mutant| (mutant, value)),
            Err(error) => {
                self.abort();
                Err(error.into())
            }
        }
    }

    fn abort(self) {}
}

pub struct Replace<'a, M, G>
where
    M: Mutate<Mutant = Mesh<G>>,
    G: 'a + Geometry,
{
    mutation: Option<(&'a mut Mesh<G>, M)>,
}

impl<'a, M, G> Replace<'a, M, G>
where
    M: Mutate<Mutant = Mesh<G>>,
    G: 'a + Geometry,
{
    pub fn replace(mesh: <Self as Mutate>::Mutant, replacement: Mesh<G>) -> Self {
        let mutant = mem::replace(mesh, replacement);
        Replace {
            mutation: Some((mesh, M::mutate(mutant))),
        }
    }

    fn drain(&mut self) -> (&'a mut Mesh<G>, M) {
        self.mutation.take().unwrap()
    }

    fn drain_and_commit(&mut self) -> Result<<Self as Mutate>::Mutant, <Self as Mutate>::Error> {
        let (mesh, mutation) = self.drain();
        let mutant = mutation.commit()?;
        mem::replace(mesh, mutant);
        Ok(mesh)
    }

    fn drain_and_abort(&mut self) {
        let (_, mutation) = self.drain();
        mutation.abort();
    }
}

impl<'a, M, G> Mutate for Replace<'a, M, G>
where
    M: Mutate<Mutant = Mesh<G>>,
    G: 'a + Geometry,
{
    type Mutant = &'a mut Mesh<G>;
    type Error = <M as Mutate>::Error;

    fn mutate(mutant: Self::Mutant) -> Self {
        Self::replace(mutant, Mesh::empty())
    }

    fn commit(mut self) -> Result<<Self as Mutate>::Mutant, Self::Error> {
        let mutant = self.drain_and_commit();
        mem::forget(self);
        mutant
    }

    fn abort(mut self) {
        self.drain_and_abort();
        mem::forget(self);
    }
}

impl<'a, M, G> Deref for Replace<'a, M, G>
where
    M: Mutate<Mutant = Mesh<G>>,
    G: 'a + Geometry,
{
    type Target = M;

    fn deref(&self) -> &Self::Target {
        &self.mutation.as_ref().unwrap().1
    }
}

impl<'a, M, G> DerefMut for Replace<'a, M, G>
where
    M: Mutate<Mutant = Mesh<G>>,
    G: 'a + Geometry,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.mutation.as_mut().unwrap().1
    }
}

impl<'a, M, G> Drop for Replace<'a, M, G>
where
    M: Mutate<Mutant = Mesh<G>>,
    G: 'a + Geometry,
{
    fn drop(&mut self) {
        self.drain_and_abort();
    }
}

/// Mesh mutation.
pub struct Mutation<G>
where
    G: Geometry,
{
    mutation: FaceMutation<G>,
}

impl<G> Mutation<G>
where
    G: Geometry,
{
    pub fn replace(mesh: &mut Mesh<G>, replacement: Mesh<G>) -> Replace<Self, G> {
        Replace::replace(mesh, replacement)
    }
}

impl<G> AsStorage<Edge<G>> for Mutation<G>
where
    G: Geometry,
{
    fn as_storage(&self) -> &Storage<Edge<G>> {
        (*self.mutation).as_storage()
    }
}

impl<G> AsStorage<Face<G>> for Mutation<G>
where
    G: Geometry,
{
    fn as_storage(&self) -> &Storage<Face<G>> {
        self.mutation.as_storage()
    }
}

impl<G> AsStorage<Vertex<G>> for Mutation<G>
where
    G: Geometry,
{
    fn as_storage(&self) -> &Storage<Vertex<G>> {
        (**self.mutation).as_storage()
    }
}

impl<G> Mutate for Mutation<G>
where
    G: Geometry,
{
    type Mutant = Mesh<G>;
    type Error = Error;

    fn mutate(mutant: Self::Mutant) -> Self {
        Mutation {
            mutation: FaceMutation::mutate(mutant.into()),
        }
    }

    fn commit(self) -> Result<Self::Mutant, Self::Error> {
        self.mutation.commit().map(|core| core.into())
    }
}

impl<G> Container for Mutation<G>
where
    G: Geometry,
{
    type Contract = Indeterminate;
}

impl<G> Deref for Mutation<G>
where
    G: Geometry,
{
    type Target = FaceMutation<G>;

    fn deref(&self) -> &Self::Target {
        &self.mutation
    }
}

impl<G> DerefMut for Mutation<G>
where
    G: Geometry,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.mutation
    }
}
