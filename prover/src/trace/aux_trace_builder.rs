use crypto::{ElementHasher, RandomCoin};
use math::FieldElement;

use crate::matrix::ColMatrix;

/// Accesses the type of the auxiliary trace's random elements in the [`AuxTraceBuilder`].
pub type AuxRandElements<ATB, E> = <ATB as AuxTraceBuilder>::AuxRandElements<E>;

/// Accesses the type of the parameters used to build the auxiliary trace in [`AuxTraceBuilder`].
pub type AuxParams<ATB> = <ATB as AuxTraceBuilder>::AuxParams;

/// Accesses the type of the auxiliary proof in the [`AuxTraceBuilder`].
pub type AuxProof<ATB> = <ATB as AuxTraceBuilder>::AuxProof;

/// Holds the auxiliary trace, random elements, and optionally, an auxiliary proof.
///
/// This type is returned by [`AuxTraceBuilder::build_aux_trace`].
pub struct AuxTraceWithMetadata<E: FieldElement, AuxRandEles, AuxProof> {
    pub aux_trace: ColMatrix<E>,
    pub aux_rand_eles: AuxRandEles,
    pub aux_proof: Option<AuxProof>,
}

/// Defines the interface for building the auxiliary trace.
pub trait AuxTraceBuilder {
    /// A type defining the random elements used in constructing the auxiliary trace.
    type AuxRandElements<E: Send + Sync>;

    /// A type defining the parameters used in building the auxiliary trace.
    type AuxParams;

    /// Optionally, an extra proof object. If not needed, set to `()`.
    ///
    /// This is useful in cases where part of the auxiliary trace is proved outside of the STARK
    /// proof system.
    type AuxProof;

    /// Builds the auxiliary trace.
    fn build_aux_trace<E, Hasher>(
        &mut self,
        main_trace: &ColMatrix<E::BaseField>,
        aux_params: Self::AuxParams,
        transcript: &mut impl RandomCoin<BaseField = E::BaseField, Hasher = Hasher>,
    ) -> AuxTraceWithMetadata<E, Self::AuxRandElements<E>, Self::AuxProof>
    where
        E: FieldElement,
        Hasher: ElementHasher<BaseField = E::BaseField>;
}
