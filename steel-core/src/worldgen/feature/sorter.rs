use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use rustc_hash::FxHashMap;
use steel_registry::RegistryEntry as _;
use steel_registry::biome::BiomeRef;
use steel_registry::feature::{PlacedFeatureData, PlacedFeatureEntryRef};

/// One placed feature a biome's generation settings list for a step.
///
/// Vanilla parity: a `Holder<PlacedFeature>`, which can be a registry reference
/// or a direct value. The flat generator's `FILL_LAYER` layers are the only
/// direct ones Steel builds, and they exist only for the world that built them,
/// so they are carried here rather than registered.
#[derive(Clone, Debug)]
pub(crate) enum FeatureEntry {
    /// A registered placed feature.
    Registered(PlacedFeatureEntryRef),
    /// A placed feature a generator built for itself, with the identity the
    /// generator gave it.
    Inline(usize, Arc<PlacedFeatureData>),
}

impl FeatureEntry {
    fn identity(&self) -> FeatureIdentity {
        match self {
            Self::Registered(feature) => {
                let Some(id) = feature.try_id() else {
                    panic!("placed feature {} is not registered", feature.key);
                };
                FeatureIdentity::Registered(id)
            }
            Self::Inline(identity, _) => FeatureIdentity::Inline(*identity),
        }
    }
}

/// The placed features one biome contributes, step by step.
///
/// Vanilla parity: `BiomeGenerationSettings.features()` for that biome, after
/// whatever the generator does to it -- the flat generator rewrites its own
/// biome's list in `adjustGenerationSettings`.
pub(crate) struct BiomeFeatures {
    /// The biome these features belong to.
    pub(crate) biome: BiomeRef,
    /// Features per decoration step.
    pub(crate) steps: Vec<Vec<FeatureEntry>>,
}

/// Identity of one placed feature, which is what vanilla's index lookup is
/// built on.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum FeatureIdentity {
    Registered(usize),
    Inline(usize),
}

/// Cached vanilla ordering for all placed features reachable from a biome source.
#[derive(Debug)]
pub(super) struct FeatureSorter {
    steps: Box<[FeatureStepData]>,
}

#[derive(Debug)]
pub(super) struct FeatureStepData {
    features: Box<[FeatureEntry]>,
    index_by_identity: FxHashMap<FeatureIdentity, usize>,
    feature_indices_by_biome_id: FxHashMap<usize, Box<[usize]>>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct FeatureVertex {
    step: usize,
    order: usize,
    identity: FeatureIdentity,
}

impl Ord for FeatureVertex {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.step, self.order, self.identity).cmp(&(other.step, other.order, other.identity))
    }
}

impl PartialOrd for FeatureVertex {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl FeatureSorter {
    #[must_use]
    pub(super) fn build(sources: &[BiomeFeatures]) -> Self {
        let mut feature_order_by_identity = FxHashMap::default();
        let mut next_feature_order = 0usize;
        let mut edges = BTreeMap::<FeatureVertex, BTreeSet<FeatureVertex>>::new();

        for source in sources {
            let mut biome_features = Vec::new();

            for (step, feature_stage) in source.steps.iter().enumerate() {
                for entry in feature_stage {
                    let identity = entry.identity();
                    let feature_order =
                        *feature_order_by_identity
                            .entry(identity)
                            .or_insert_with(|| {
                                let order = next_feature_order;
                                next_feature_order += 1;
                                order
                            });

                    let vertex = FeatureVertex {
                        step,
                        order: feature_order,
                        identity,
                    };
                    edges.entry(vertex).or_default();
                    biome_features.push(vertex);
                }
            }

            for feature_pair in biome_features.windows(2) {
                edges
                    .entry(feature_pair[0])
                    .or_default()
                    .insert(feature_pair[1]);
            }
        }

        let sorted_features = Self::topological_sort(&edges);
        Self::from_sorted_features(&sorted_features, sources)
    }

    #[must_use]
    pub(super) fn step_count(&self) -> usize {
        self.steps.len()
    }

    pub(super) fn step(&self, step: usize) -> Option<&FeatureStepData> {
        self.steps.get(step)
    }

    fn topological_sort(
        edges: &BTreeMap<FeatureVertex, BTreeSet<FeatureVertex>>,
    ) -> Vec<FeatureVertex> {
        let mut sorted = Vec::with_capacity(edges.len());
        let mut discovered = BTreeSet::new();
        let mut visiting = BTreeSet::new();
        let vertices = edges.keys().copied().collect::<Vec<_>>();

        for vertex in vertices {
            assert!(
                !Self::visit(vertex, edges, &mut discovered, &mut visiting, &mut sorted),
                "biome decoration placed-feature order contains a cycle"
            );
        }

        sorted.reverse();
        sorted
    }

    fn visit(
        vertex: FeatureVertex,
        edges: &BTreeMap<FeatureVertex, BTreeSet<FeatureVertex>>,
        discovered: &mut BTreeSet<FeatureVertex>,
        visiting: &mut BTreeSet<FeatureVertex>,
        sorted: &mut Vec<FeatureVertex>,
    ) -> bool {
        if discovered.contains(&vertex) {
            return false;
        }
        if !visiting.insert(vertex) {
            return true;
        }

        if let Some(neighbors) = edges.get(&vertex) {
            for &neighbor in neighbors {
                if Self::visit(neighbor, edges, discovered, visiting, sorted) {
                    return true;
                }
            }
        }

        visiting.remove(&vertex);
        discovered.insert(vertex);
        sorted.push(vertex);
        false
    }

    #[must_use]
    fn from_sorted_features(sorted_features: &[FeatureVertex], sources: &[BiomeFeatures]) -> Self {
        let Some(max_step) = sorted_features.iter().map(|feature| feature.step).max() else {
            return Self {
                steps: Box::new([]),
            };
        };

        let entry_by_identity: FxHashMap<FeatureIdentity, FeatureEntry> = sources
            .iter()
            .flat_map(|source| source.steps.iter().flatten())
            .map(|entry| (entry.identity(), entry.clone()))
            .collect();

        let mut steps = Vec::with_capacity(max_step + 1);
        for step in 0..=max_step {
            let mut features = Vec::new();
            let mut index_by_identity = FxHashMap::default();

            for feature in sorted_features
                .iter()
                .filter(|feature| feature.step == step)
            {
                let Some(entry) = entry_by_identity.get(&feature.identity) else {
                    panic!("feature sorter references a placed feature no biome listed");
                };
                let index = features.len();
                features.push(entry.clone());
                index_by_identity.insert(feature.identity, index);
            }

            steps.push(FeatureStepData {
                features: features.into_boxed_slice(),
                index_by_identity,
                feature_indices_by_biome_id: FxHashMap::default(),
            });
        }

        for source in sources {
            let Some(biome_id) = source.biome.try_id() else {
                panic!("possible biome {} is not registered", source.biome.key);
            };

            for (step, feature_stage) in source.steps.iter().enumerate() {
                let Some(step_data) = steps.get_mut(step) else {
                    continue;
                };

                let mut indices = Vec::with_capacity(feature_stage.len());
                for entry in feature_stage {
                    let Some(feature_index) = step_data.feature_index(entry.identity()) else {
                        panic!(
                            "a placed feature from biome {} was not included in decoration step {step}",
                            source.biome.key
                        );
                    };
                    indices.push(feature_index);
                }

                if indices.is_empty() {
                    continue;
                }

                indices.sort_unstable();
                indices.dedup();
                step_data
                    .feature_indices_by_biome_id
                    .insert(biome_id, indices.into_boxed_slice());
            }
        }

        Self {
            steps: steps.into_boxed_slice(),
        }
    }
}

impl FeatureStepData {
    fn feature_index(&self, identity: FeatureIdentity) -> Option<usize> {
        self.index_by_identity.get(&identity).copied()
    }

    pub(super) fn feature(&self, index: usize) -> Option<&FeatureEntry> {
        self.features.get(index)
    }

    pub(super) fn feature_indices_for_biome(&self, biome_id: usize) -> Option<&[usize]> {
        self.feature_indices_by_biome_id
            .get(&biome_id)
            .map(Box::as_ref)
    }
}
