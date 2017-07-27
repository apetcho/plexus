use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

use graph::geometry::Geometry;
use graph::mesh::{self, Mesh};
use graph::storage::{FaceKey, Key, OpaqueKey};

// TODO: Generalize this pairing of a ref to a mesh and a key for topology
//       within the mesh.

pub struct Face<M, G, K>
where
    M: AsRef<Mesh<G, K>>,
    G: Geometry,
    K: Key,
{
    mesh: M,
    key: FaceKey<K>,
    phantom: PhantomData<G>,
}

impl<M, G, K> Face<M, G, K>
where
    M: AsRef<Mesh<G, K>>,
    G: Geometry,
    K: Key,
{
    // This borrows a mesh and should always be generated by that same mesh.
    // This means that if dereferencing fails, something has gone horribly
    // wrong and panicing is probably the correct behavior.
    pub(super) fn new(mesh: M, face: FaceKey<K>) -> Self {
        Face {
            mesh: mesh,
            key: face,
            phantom: PhantomData,
        }
    }
}

impl<M, G, K> Face<M, G, K>
where
    M: AsRef<Mesh<G, K>> + AsMut<Mesh<G, K>>,
    G: Geometry,
    K: Key,
{
}

impl<'a, M, G, K> Deref for Face<&'a M, G, K>
where
    M: AsRef<Mesh<G, K>>,
    G: Geometry,
    K: Key,
{
    type Target = mesh::Face<G::FaceData, K>;

    fn deref(&self) -> &Self::Target {
        self.mesh.as_ref().faces.get(&self.key.to_inner()).unwrap()
    }
}

impl<'a, M, G, K> Deref for Face<&'a mut M, G, K>
where
    M: AsRef<Mesh<G, K>> + AsMut<Mesh<G, K>>,
    G: Geometry,
    K: Key,
{
    type Target = mesh::Face<G::FaceData, K>;

    fn deref(&self) -> &Self::Target {
        self.mesh.as_ref().faces.get(&self.key.to_inner()).unwrap()
    }
}

impl<'a, M, G, K> DerefMut for Face<&'a mut M, G, K>
where
    M: AsRef<Mesh<G, K>> + AsMut<Mesh<G, K>>,
    G: Geometry,
    K: Key,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.mesh
            .as_mut()
            .faces
            .get_mut(&self.key.to_inner())
            .unwrap()
    }
}
