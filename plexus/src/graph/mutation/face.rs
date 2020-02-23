use itertools::Itertools;
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use theon::space::{EuclideanSpace, Vector};
use theon::AsPosition;

use crate::graph::borrow::Reborrow;
use crate::graph::core::{Core, Fuse, OwnedCore, RefCore};
use crate::graph::geometry::{Geometric, Geometry, GraphGeometry, VertexPosition};
use crate::graph::mutation::edge::{self, ArcBridgeCache, EdgeMutation};
use crate::graph::mutation::{Consistent, Mutable, Mutation};
use crate::graph::storage::alias::*;
use crate::graph::storage::key::{ArcKey, FaceKey, VertexKey};
use crate::graph::storage::payload::{Arc, Face, Vertex};
use crate::graph::storage::{AsStorage, StorageProxy};
use crate::graph::view::edge::ArcView;
use crate::graph::view::face::FaceView;
use crate::graph::view::vertex::VertexView;
use crate::graph::view::{ClosedView, View};
use crate::graph::GraphError;
use crate::transact::Transact;
use crate::{DynamicArity, IteratorExt as _};

type Mutant<G> = OwnedCore<G>;

pub struct FaceMutation<M>
where
    M: Geometric,
{
    inner: EdgeMutation<M>,
    storage: StorageProxy<Face<Geometry<M>>>,
}

impl<M, G> FaceMutation<M>
where
    M: Geometric<Geometry = G>,
    G: GraphGeometry,
{
    fn core(&self) -> RefCore<G> {
        Core::empty()
            .fuse(self.as_vertex_storage())
            .fuse(self.as_arc_storage())
            .fuse(self.as_edge_storage())
            .fuse(self.as_face_storage())
    }

    pub fn insert_face(
        &mut self,
        vertices: &[VertexKey],
        geometry: (G::Arc, G::Face),
    ) -> Result<FaceKey, GraphError> {
        let cache = FaceInsertCache::snapshot(&self.core(), vertices, geometry)?;
        self.insert_face_with_cache(cache)
    }

    pub fn insert_face_with_cache(
        &mut self,
        cache: FaceInsertCache<G>,
    ) -> Result<FaceKey, GraphError> {
        let FaceInsertCache {
            vertices,
            connectivity,
            geometry,
            ..
        } = cache;
        // Insert edges and collect the interior arcs.
        let arcs = vertices
            .iter()
            .cloned()
            .perimeter()
            .map(|(a, b)| {
                self.get_or_insert_edge_with((a, b), || geometry.0)
                    .map(|(_, (ab, _))| ab)
            })
            .collect::<Result<Vec<_>, _>>()?;
        // Insert the face.
        let face = self.storage.insert(Face::new(arcs[0], geometry.1));
        self.connect_face_interior(&arcs, face)?;
        self.connect_face_exterior(&arcs, connectivity)?;
        Ok(face)
    }

    // TODO: Should there be a distinction between `connect_face_to_edge` and
    //       `connect_edge_to_face`?
    pub fn connect_face_to_arc(&mut self, ab: ArcKey, abc: FaceKey) -> Result<(), GraphError> {
        self.storage
            .get_mut(&abc)
            .ok_or_else(|| GraphError::TopologyNotFound)?
            .arc = ab;
        Ok(())
    }

    fn connect_face_interior(&mut self, arcs: &[ArcKey], face: FaceKey) -> Result<(), GraphError> {
        for (ab, bc) in arcs.iter().cloned().perimeter() {
            self.connect_neighboring_arcs(ab, bc)?;
            self.connect_arc_to_face(ab, face)?;
        }
        Ok(())
    }

    fn disconnect_face_interior(&mut self, arcs: &[ArcKey]) -> Result<(), GraphError> {
        for ab in arcs {
            self.disconnect_arc_from_face(*ab)?;
        }
        Ok(())
    }

    fn connect_face_exterior(
        &mut self,
        arcs: &[ArcKey],
        connectivity: (
            HashMap<VertexKey, Vec<ArcKey>>,
            HashMap<VertexKey, Vec<ArcKey>>,
        ),
    ) -> Result<(), GraphError> {
        let (incoming, outgoing) = connectivity;
        for ab in arcs.iter().cloned() {
            let (a, b) = ab.into();
            let ba = ab.into_opposite();
            let neighbors = {
                let core = &self.core();
                if View::bind(core, ba)
                    .map(ArcView::from)
                    .ok_or_else(|| GraphError::TopologyMalformed)?
                    .is_boundary_arc()
                {
                    // The next arc of BA is the outgoing arc of the destination
                    // vertex A that is also a boundary arc or, if there is no
                    // such outgoing arc, the next exterior arc of the face. The
                    // previous arc is similar.
                    let ax = outgoing[&a]
                        .iter()
                        .cloned()
                        .flat_map(|ax| View::bind(core, ax).map(ArcView::from))
                        .find(|next| next.is_boundary_arc())
                        .or_else(|| {
                            View::bind(core, ab)
                                .map(ArcView::from)
                                .and_then(|arc| arc.into_reachable_previous_arc())
                                .and_then(|previous| previous.into_reachable_opposite_arc())
                        })
                        .map(|next| next.key());
                    let xb = incoming[&b]
                        .iter()
                        .cloned()
                        .flat_map(|xb| View::bind(core, xb).map(ArcView::from))
                        .find(|previous| previous.is_boundary_arc())
                        .or_else(|| {
                            View::bind(core, ab)
                                .map(ArcView::from)
                                .and_then(|arc| arc.into_reachable_next_arc())
                                .and_then(|next| next.into_reachable_opposite_arc())
                        })
                        .map(|previous| previous.key());
                    ax.into_iter().zip(xb.into_iter()).next()
                }
                else {
                    None
                }
            };
            if let Some((ax, xb)) = neighbors {
                self.connect_neighboring_arcs(ba, ax)?;
                self.connect_neighboring_arcs(xb, ba)?;
            }
        }
        Ok(())
    }
}

impl<M, G> AsStorage<Face<G>> for FaceMutation<M>
where
    M: Geometric<Geometry = G>,
    G: GraphGeometry,
{
    fn as_storage(&self) -> &StorageProxy<Face<G>> {
        &self.storage
    }
}

// TODO: This is a hack. Replace this with delegation.
impl<M> Deref for FaceMutation<M>
where
    M: Geometric,
{
    type Target = EdgeMutation<M>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<M> DerefMut for FaceMutation<M>
where
    M: Geometric,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<M, G> From<Mutant<G>> for FaceMutation<M>
where
    M: Geometric<Geometry = G>,
    G: GraphGeometry,
{
    fn from(core: Mutant<G>) -> Self {
        let (vertices, arcs, edges, faces) = core.unfuse();
        FaceMutation {
            storage: faces,
            inner: Core::empty().fuse(vertices).fuse(arcs).fuse(edges).into(),
        }
    }
}

impl<M, G> Transact<Mutant<G>> for FaceMutation<M>
where
    M: Geometric<Geometry = G>,
    G: GraphGeometry,
{
    type Output = Mutant<G>;
    type Error = GraphError;

    fn commit(self) -> Result<Self::Output, Self::Error> {
        let FaceMutation {
            inner,
            storage: faces,
            ..
        } = self;
        inner.commit().map(move |core| core.fuse(faces))
    }
}

pub struct FaceInsertCache<'a, G>
where
    G: GraphGeometry,
{
    vertices: &'a [VertexKey],
    connectivity: (
        HashMap<VertexKey, Vec<ArcKey>>,
        HashMap<VertexKey, Vec<ArcKey>>,
    ),
    geometry: (G::Arc, G::Face),
}

impl<'a, G> FaceInsertCache<'a, G>
where
    G: GraphGeometry,
{
    pub fn snapshot<M>(
        storage: M,
        keys: &'a [VertexKey],
        geometry: (G::Arc, G::Face),
    ) -> Result<Self, GraphError>
    where
        M: Reborrow,
        M::Target:
            AsStorage<Arc<G>> + AsStorage<Face<G>> + AsStorage<Vertex<G>> + Geometric<Geometry = G>,
    {
        let arity = keys.len();
        let set = keys.iter().cloned().collect::<HashSet<_>>();
        if set.len() != arity {
            // Vertex keys are not unique.
            return Err(GraphError::TopologyMalformed);
        }

        let storage = storage.reborrow();
        let vertices = keys
            .iter()
            .cloned()
            .flat_map(|key| View::bind_into(storage, key))
            .collect::<SmallVec<[VertexView<_>; 4]>>();
        if vertices.len() != arity {
            // Vertex keys refer to nonexistent vertices.
            return Err(GraphError::TopologyNotFound);
        }
        for (previous, next) in keys
            .iter()
            .cloned()
            .perimeter()
            .map(|keys| View::bind(storage, keys.into()).map(ArcView::from))
            .perimeter()
        {
            if let Some(previous) = previous {
                if previous.face.is_some() {
                    // An interior arc is already occuppied by a face.
                    return Err(GraphError::TopologyConflict);
                }
                // Let the previous arc be AB and the next arc be BC. The
                // vertices A, B, and C lie within the implied ring in order.
                //
                // If BC does not exist and AB is neighbors with some arc BX,
                // then X must not lie within the implied ring (the ordered set
                // of vertices given to this function). If X is within the path,
                // then BX must bisect the implied ring (because X cannot be C).
                if next.is_none() {
                    if let Some(next) = previous.reachable_next_arc() {
                        let (_, destination) = next.key().into();
                        if set.contains(&destination) {
                            return Err(GraphError::TopologyConflict);
                        }
                    }
                }
            }
        }

        let mut incoming = HashMap::with_capacity(arity);
        let mut outgoing = HashMap::with_capacity(arity);
        for vertex in vertices {
            let key = vertex.key();
            incoming.insert(key, vertex.reachable_incoming_arcs().keys().collect());
            outgoing.insert(key, vertex.reachable_outgoing_arcs().keys().collect());
        }
        Ok(FaceInsertCache {
            vertices: keys,
            connectivity: (incoming, outgoing),
            geometry,
        })
    }
}

pub struct FaceRemoveCache<G>
where
    G: GraphGeometry,
{
    abc: FaceKey,
    arcs: Vec<ArcKey>,
    phantom: PhantomData<G>,
}

impl<G> FaceRemoveCache<G>
where
    G: GraphGeometry,
{
    // TODO: Should this require consistency?
    pub fn snapshot<M>(storage: M, abc: FaceKey) -> Result<Self, GraphError>
    where
        M: Reborrow,
        M::Target: AsStorage<Arc<G>>
            + AsStorage<Face<G>>
            + AsStorage<Vertex<G>>
            + Consistent
            + Geometric<Geometry = G>,
    {
        let face = View::bind(storage, abc)
            .map(FaceView::from)
            .ok_or_else(|| GraphError::TopologyNotFound)?;
        let arcs = face.interior_arcs().map(|arc| arc.key()).collect();
        Ok(FaceRemoveCache {
            abc,
            arcs,
            phantom: PhantomData,
        })
    }
}

pub struct FaceSplitCache<G>
where
    G: GraphGeometry,
{
    cache: FaceRemoveCache<G>,
    left: Vec<VertexKey>,
    right: Vec<VertexKey>,
    geometry: G::Face,
}

impl<G> FaceSplitCache<G>
where
    G: GraphGeometry,
{
    pub fn snapshot<M>(
        storage: M,
        abc: FaceKey,
        source: VertexKey,
        destination: VertexKey,
    ) -> Result<Self, GraphError>
    where
        M: Reborrow,
        M::Target: AsStorage<Arc<G>>
            + AsStorage<Face<G>>
            + AsStorage<Vertex<G>>
            + Consistent
            + Geometric<Geometry = G>,
    {
        let storage = storage.reborrow();
        let face = View::bind(storage, abc)
            .map(FaceView::from)
            .ok_or_else(|| GraphError::TopologyNotFound)?;
        face.ring()
            .distance(source.into(), destination.into())
            .and_then(|distance| {
                if distance <= 1 {
                    Err(GraphError::TopologyMalformed)
                }
                else {
                    Ok(())
                }
            })?;
        let perimeter = face
            .vertices()
            .map(|vertex| vertex.key())
            .collect::<Vec<_>>()
            .into_iter()
            .cycle();
        let left = perimeter
            .clone()
            .tuple_windows()
            .skip_while(|(_, b)| *b != source)
            .take_while(|(a, _)| *a != destination)
            .map(|(_, b)| b)
            .collect::<Vec<_>>();
        let right = perimeter
            .tuple_windows()
            .skip_while(|(_, b)| *b != destination)
            .take_while(|(a, _)| *a != source)
            .map(|(_, b)| b)
            .collect::<Vec<_>>();
        Ok(FaceSplitCache {
            cache: FaceRemoveCache::snapshot(storage, abc)?,
            left,
            right,
            geometry: face.geometry,
        })
    }
}

pub struct FacePokeCache<G>
where
    G: GraphGeometry,
{
    vertices: Vec<VertexKey>,
    geometry: G::Vertex,
    cache: FaceRemoveCache<G>,
}

impl<G> FacePokeCache<G>
where
    G: GraphGeometry,
{
    pub fn snapshot<M>(storage: M, abc: FaceKey, geometry: G::Vertex) -> Result<Self, GraphError>
    where
        M: Reborrow,
        M::Target: AsStorage<Arc<G>>
            + AsStorage<Face<G>>
            + AsStorage<Vertex<G>>
            + Consistent
            + Geometric<Geometry = G>,
    {
        let storage = storage.reborrow();
        let vertices = View::bind(storage, abc)
            .map(FaceView::from)
            .ok_or_else(|| GraphError::TopologyNotFound)?
            .vertices()
            .map(|vertex| vertex.key())
            .collect();
        Ok(FacePokeCache {
            vertices,
            geometry,
            cache: FaceRemoveCache::snapshot(storage, abc)?,
        })
    }
}

pub struct FaceBridgeCache<G>
where
    G: GraphGeometry,
{
    source: SmallVec<[ArcKey; 4]>,
    destination: SmallVec<[ArcKey; 4]>,
    cache: (FaceRemoveCache<G>, FaceRemoveCache<G>),
}

impl<G> FaceBridgeCache<G>
where
    G: GraphGeometry,
{
    pub fn snapshot<M>(
        storage: M,
        source: FaceKey,
        destination: FaceKey,
    ) -> Result<Self, GraphError>
    where
        M: Reborrow,
        M::Target: AsStorage<Arc<G>>
            + AsStorage<Face<G>>
            + AsStorage<Vertex<G>>
            + Consistent
            + Geometric<Geometry = G>,
    {
        let storage = storage.reborrow();
        let cache = (
            FaceRemoveCache::snapshot(storage, source)?,
            FaceRemoveCache::snapshot(storage, destination)?,
        );
        // Ensure that the opposite face exists and has the same arity.
        let source = View::bind(storage, source)
            .map(FaceView::from)
            .ok_or_else(|| GraphError::TopologyNotFound)?;
        let destination = View::bind(storage, destination)
            .map(FaceView::from)
            .ok_or_else(|| GraphError::TopologyNotFound)?;
        if source.arity() != destination.arity() {
            return Err(GraphError::ArityNonUniform);
        }
        Ok(FaceBridgeCache {
            source: source.interior_arcs().map(|arc| arc.key()).collect(),
            destination: destination.interior_arcs().map(|arc| arc.key()).collect(),
            cache,
        })
    }
}

pub struct FaceExtrudeCache<G>
where
    G: GraphGeometry,
{
    sources: Vec<VertexKey>,
    destinations: Vec<G::Vertex>,
    geometry: G::Face,
    cache: FaceRemoveCache<G>,
}
impl<G> FaceExtrudeCache<G>
where
    G: GraphGeometry,
{
    pub fn snapshot<M>(
        storage: M,
        abc: FaceKey,
        translation: Vector<VertexPosition<G>>,
    ) -> Result<Self, GraphError>
    where
        M: Reborrow,
        M::Target: AsStorage<Arc<G>>
            + AsStorage<Face<G>>
            + AsStorage<Vertex<G>>
            + Consistent
            + Geometric<Geometry = G>,
        G::Vertex: AsPosition,
        VertexPosition<G>: EuclideanSpace,
    {
        let storage = storage.reborrow();
        let cache = FaceRemoveCache::snapshot(storage, abc)?;
        let face = View::bind(storage, abc)
            .map(FaceView::from)
            .ok_or_else(|| GraphError::TopologyNotFound)?;

        let sources = face.vertices().map(|vertex| vertex.key()).collect();
        let destinations = face
            .vertices()
            .map(|vertex| {
                let mut geometry = vertex.geometry;
                geometry.transform(|position| *position + translation);
                geometry
            })
            .collect();
        Ok(FaceExtrudeCache {
            sources,
            destinations,
            geometry: face.geometry,
            cache,
        })
    }
}

// TODO: Does this require a cache (or consistency)?
// TODO: This may need to be more destructive to maintain consistency. Edges,
//       arcs, and vertices may also need to be removed.
pub fn remove_with_cache<M, N, G>(
    mut mutation: N,
    cache: FaceRemoveCache<G>,
) -> Result<Face<G>, GraphError>
where
    N: AsMut<Mutation<M>>,
    M: Mutable<Geometry = G>,
    G: GraphGeometry,
{
    let FaceRemoveCache { abc, arcs, .. } = cache;
    mutation.as_mut().disconnect_face_interior(&arcs)?;
    let face = mutation
        .as_mut()
        .storage
        .remove(&abc)
        .ok_or_else(|| GraphError::TopologyNotFound)?;
    Ok(face)
}

pub fn split_with_cache<M, N, G>(
    mut mutation: N,
    cache: FaceSplitCache<G>,
) -> Result<ArcKey, GraphError>
where
    N: AsMut<Mutation<M>>,
    M: Mutable<Geometry = G>,
    G: GraphGeometry,
{
    let FaceSplitCache {
        cache,
        left,
        right,
        geometry,
        ..
    } = cache;
    remove_with_cache(mutation.as_mut(), cache)?;
    mutation
        .as_mut()
        .insert_face(&left, (Default::default(), geometry))?;
    mutation
        .as_mut()
        .insert_face(&right, (Default::default(), geometry))?;
    Ok((left[0], right[0]).into())
}

pub fn poke_with_cache<M, N, G>(
    mut mutation: N,
    cache: FacePokeCache<G>,
) -> Result<VertexKey, GraphError>
where
    N: AsMut<Mutation<M>>,
    M: Mutable<Geometry = G>,
    G: GraphGeometry,
{
    let FacePokeCache {
        vertices,
        geometry,
        cache,
    } = cache;
    let face = remove_with_cache(mutation.as_mut(), cache)?;
    let c = mutation.as_mut().insert_vertex(geometry);
    for (a, b) in vertices.into_iter().perimeter() {
        mutation
            .as_mut()
            .insert_face(&[a, b, c], (Default::default(), face.geometry))?;
    }
    Ok(c)
}

pub fn bridge_with_cache<M, N, G>(
    mut mutation: N,
    cache: FaceBridgeCache<G>,
) -> Result<(), GraphError>
where
    N: AsMut<Mutation<M>>,
    M: Mutable<Geometry = G>,
    G: GraphGeometry,
{
    let FaceBridgeCache {
        source,
        destination,
        cache,
    } = cache;
    // Remove the source and destination faces. Pair the topology with edge
    // geometry for the source face.
    remove_with_cache(mutation.as_mut(), cache.0)?;
    remove_with_cache(mutation.as_mut(), cache.1)?;
    // TODO: Is it always correct to reverse the order of the opposite face's
    //       arcs?
    // Re-insert the arcs of the faces and bridge the mutual arcs.
    for (ab, cd) in source.into_iter().zip(destination.into_iter().rev()) {
        // TODO: It should NOT be necessary to construct a `Core` to pass to
        //       `snapshot` here, but using `mutation.as_mut()` causes the
        //       compiler to complain that `Mutation<M>` does not implement
        //       `Reborrow`. It doesn't, but `mutation.as_mut()` returns
        //       `&mut Mutation<M>`, which implements that trait!
        let mutation = mutation.as_mut();
        let core = Core::empty()
            .fuse(mutation.as_vertex_storage())
            .fuse(mutation.as_arc_storage())
            .fuse(mutation.as_face_storage());
        let cache = ArcBridgeCache::snapshot(&core, ab, cd)?;
        edge::bridge_with_cache(mutation, cache)?;
    }
    // TODO: Is there any reasonable topology this can return?
    Ok(())
}

pub fn extrude_with_cache<M, N, G>(
    mut mutation: N,
    cache: FaceExtrudeCache<G>,
) -> Result<FaceKey, GraphError>
where
    N: AsMut<Mutation<M>>,
    M: Mutable<Geometry = G>,
    G: GraphGeometry,
{
    let FaceExtrudeCache {
        sources,
        destinations,
        geometry,
        cache,
    } = cache;
    remove_with_cache(mutation.as_mut(), cache)?;
    let destinations = destinations
        .into_iter()
        .map(|a| mutation.as_mut().insert_vertex(a))
        .collect::<Vec<_>>();
    // Use the keys for the existing vertices and the translated geometries to
    // construct the extruded face and its connective faces.
    let extrusion = mutation
        .as_mut()
        .insert_face(&destinations, (Default::default(), geometry))?;
    for ((a, c), (b, d)) in sources
        .into_iter()
        .zip(destinations.into_iter())
        .perimeter()
    {
        // TODO: Split these faces to form triangles.
        mutation
            .as_mut()
            .insert_face(&[a, b, d, c], (Default::default(), geometry))?;
    }
    Ok(extrusion)
}
