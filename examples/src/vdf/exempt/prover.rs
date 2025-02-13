// Copyright (c) Facebook, Inc. and its affiliates.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use super::{
    BaseElement, DefaultConstraintEvaluator, DefaultRandomCoin, DefaultTraceLde, ElementHasher,
    FieldElement, PhantomData, ProofOptions, Prover, Trace, TraceTable, VdfAir, VdfInputs,
    FORTY_TWO, INV_ALPHA,
};

// VDF PROVER
// ================================================================================================

pub struct VdfProver<H: ElementHasher> {
    options: ProofOptions,
    _hasher: PhantomData<H>,
}

impl<H: ElementHasher> VdfProver<H> {
    pub fn new(options: ProofOptions) -> Self {
        Self {
            options,
            _hasher: PhantomData,
        }
    }

    pub fn build_trace(seed: BaseElement, n: usize) -> TraceTable<BaseElement> {
        let mut trace = Vec::with_capacity(n);
        let mut state = seed;

        trace.push(state);
        for _ in 0..(n - 2) {
            state = (state - FORTY_TWO).exp(INV_ALPHA);
            trace.push(state);
        }

        // put garbage value into the last step
        trace.push(BaseElement::new(123));

        TraceTable::init(vec![trace])
    }
}

impl<H: ElementHasher> Prover for VdfProver<H>
where
    H: ElementHasher<BaseField = BaseElement>,
{
    type BaseField = BaseElement;
    type Air = VdfAir;
    type Trace = TraceTable<BaseElement>;
    type HashFn = H;
    type RandomCoin = DefaultRandomCoin<Self::HashFn>;
    type TraceLde<E: FieldElement<BaseField = Self::BaseField>> = DefaultTraceLde<E, Self::HashFn>;
    type ConstraintEvaluator<'a, E: FieldElement<BaseField = Self::BaseField>> =
        DefaultConstraintEvaluator<'a, Self::Air, E>;

    fn get_pub_inputs(&self, trace: &Self::Trace) -> VdfInputs {
        // the result is read from the second to last step because the last last step contains
        // garbage
        let second_to_last_step = trace.length() - 2;
        VdfInputs {
            seed: trace.get(0, 0),
            result: trace.get(0, second_to_last_step),
        }
    }

    fn options(&self) -> &ProofOptions {
        &self.options
    }

    fn new_evaluator<'a, E>(
        &self,
        air: &'a Self::Air,
        aux_rand_elements: winterfell::AuxTraceRandElements<E>,
        composition_coefficients: winterfell::ConstraintCompositionCoefficients<E>,
    ) -> Self::ConstraintEvaluator<'a, E>
    where
        E: FieldElement<BaseField = Self::BaseField>,
    {
        DefaultConstraintEvaluator::new(air, aux_rand_elements, composition_coefficients)
    }
}
