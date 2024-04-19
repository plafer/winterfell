// Copyright (c) Facebook, Inc. and its affiliates.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use super::{
    apply_rescue_round_parallel, rescue::STATE_WIDTH, BaseElement, DefaultRandomCoin,
    ElementHasher, FieldElement, PhantomData, ProofOptions, Prover, PublicInputs, RapTraceTable,
    RescueRapsAir, CYCLE_LENGTH, NUM_HASH_ROUNDS,
};
use core_utils::uninit_vector;
use winterfell::{
    crypto::RandomCoin, matrix::ColMatrix, AuxTraceBuilder, AuxTraceWithMetadata,
    ConstraintCompositionCoefficients, DefaultConstraintEvaluator, DefaultTraceLde, StarkDomain,
    Trace, TraceInfo, TracePolyTable,
};

pub struct RescueRapsAuxTraceBuilder;

impl AuxTraceBuilder for RescueRapsAuxTraceBuilder {
    type AuxRandElements<E> = Vec<E>;

    // aux segment width
    type AuxParams = usize;

    type AuxProof = ();

    fn build_aux_trace<E, Hasher>(
        &mut self,
        main_trace: &ColMatrix<E::BaseField>,
        aux_segment_width: Self::AuxParams,
        transcript: &mut impl RandomCoin<BaseField = E::BaseField, Hasher = Hasher>,
    ) -> AuxTraceWithMetadata<E, Self::AuxRandElements<E>, Self::AuxProof>
    where
        E: FieldElement,
        Hasher: ElementHasher<BaseField = E::BaseField>,
    {
        let rand_elements = {
            let mut rand_elements = Vec::with_capacity(aux_segment_width);
            for _ in 0..aux_segment_width {
                rand_elements.push(transcript.draw().unwrap());
            }

            rand_elements
        };

        let mut current_row = unsafe { uninit_vector(main_trace.num_cols()) };
        let mut next_row = unsafe { uninit_vector(main_trace.num_cols()) };
        main_trace.read_row_into(0, &mut current_row);
        let mut aux_columns = vec![vec![E::ZERO; main_trace.num_rows()]; aux_segment_width];

        // Columns storing the copied values for the permutation argument are not necessary, but
        // help understanding the construction of RAPs and are kept for illustrative purposes.
        aux_columns[0][0] =
            rand_elements[0] * current_row[0].into() + rand_elements[1] * current_row[1].into();
        aux_columns[1][0] =
            rand_elements[0] * current_row[4].into() + rand_elements[1] * current_row[5].into();

        // Permutation argument column
        aux_columns[2][0] = E::ONE;

        for index in 1..main_trace.num_rows() {
            // At every last step before a new hash iteration,
            // copy the permuted values into the auxiliary columns
            if (index % super::CYCLE_LENGTH) == super::NUM_HASH_ROUNDS {
                main_trace.read_row_into(index, &mut current_row);
                main_trace.read_row_into(index + 1, &mut next_row);

                aux_columns[0][index] = rand_elements[0] * (next_row[0] - current_row[0]).into()
                    + rand_elements[1] * (next_row[1] - current_row[1]).into();
                aux_columns[1][index] = rand_elements[0] * (next_row[4] - current_row[4]).into()
                    + rand_elements[1] * (next_row[5] - current_row[5]).into();
            }

            let num = aux_columns[0][index - 1] + rand_elements[2];
            let denom = aux_columns[1][index - 1] + rand_elements[2];
            aux_columns[2][index] = aux_columns[2][index - 1] * num * denom.inv();
        }

        AuxTraceWithMetadata {
            aux_trace: ColMatrix::new(aux_columns),
            aux_rand_eles: rand_elements,
            aux_proof: None,
        }
    }
}

// RESCUE PROVER
// ================================================================================================
/// This example constructs a proof for correct execution of 2 hash chains simultaneously.
/// In order to demonstrate the power of RAPs, the two hash chains have seeds that are
/// permutations of each other.
pub struct RescueRapsProver<H: ElementHasher> {
    options: ProofOptions,
    _hasher: PhantomData<H>,
}

impl<H: ElementHasher> RescueRapsProver<H> {
    pub fn new(options: ProofOptions) -> Self {
        Self {
            options,
            _hasher: PhantomData,
        }
    }
    /// The parameter `seeds` is the set of seeds for the first hash chain.
    /// The parameter `permuted_seeds` is the set of seeds for the second hash chain.
    pub fn build_trace(
        &self,
        seeds: &[[BaseElement; 2]],
        permuted_seeds: &[[BaseElement; 2]],
        result: [[BaseElement; 2]; 2],
    ) -> RapTraceTable<BaseElement> {
        debug_assert_eq!(seeds.len(), permuted_seeds.len());
        // allocate memory to hold the trace table
        let trace_length = seeds.len() * CYCLE_LENGTH;
        let mut trace = RapTraceTable::new(2 * STATE_WIDTH, trace_length);
        const END_INCLUSIVE_RANGE: usize = NUM_HASH_ROUNDS - 1;

        trace.fill(
            |state| {
                // initialize original chain
                state[0] = seeds[0][0];
                state[1] = seeds[0][1];
                state[2] = BaseElement::ZERO;
                state[3] = BaseElement::ZERO;

                // initialize permuted chain
                state[4] = permuted_seeds[0][0];
                state[5] = permuted_seeds[0][1];
                state[6] = BaseElement::ZERO;
                state[7] = BaseElement::ZERO;
            },
            |step, state| {
                // execute the transition function for all steps
                //
                // for the first 14 steps in every cycle, compute a single round of
                // Rescue hash; for the remaining 2 rounds, carry over the values
                // in the first two registers of the two chains to the next step
                // and insert the additional seeds into the capacity registers
                match step % CYCLE_LENGTH {
                    0..=END_INCLUSIVE_RANGE => apply_rescue_round_parallel(state, step),
                    NUM_HASH_ROUNDS => {
                        let idx = step / CYCLE_LENGTH + 1;
                        // We don't have seeds for the final step once last hashing is done.
                        if idx < seeds.len() {
                            state[0] += seeds[idx][0];
                            state[1] += seeds[idx][1];

                            state[4] += permuted_seeds[idx][0];
                            state[5] += permuted_seeds[idx][1];
                        }
                    }
                    _ => {}
                };
            },
        );

        debug_assert_eq!(trace.get(0, trace_length - 1), result[0][0]);
        debug_assert_eq!(trace.get(1, trace_length - 1), result[0][1]);

        debug_assert_eq!(trace.get(4, trace_length - 1), result[1][0]);
        debug_assert_eq!(trace.get(5, trace_length - 1), result[1][1]);

        trace
    }
}

impl<H: ElementHasher> Prover for RescueRapsProver<H>
where
    H: ElementHasher<BaseField = BaseElement>,
{
    type AuxRandElements<E> = Vec<E>;
    type BaseField = BaseElement;
    type Air = RescueRapsAir;
    type Trace = RapTraceTable<BaseElement>;
    type HashFn = H;
    type RandomCoin = DefaultRandomCoin<Self::HashFn>;
    type TraceLde<E: FieldElement<BaseField = Self::BaseField>> = DefaultTraceLde<E, Self::HashFn>;
    type ConstraintEvaluator<'a, E: FieldElement<BaseField = Self::BaseField>> =
        DefaultConstraintEvaluator<'a, Self::Air, E>;

    fn get_pub_inputs(&self, trace: &Self::Trace) -> PublicInputs {
        let last_step = trace.length() - 1;
        PublicInputs {
            result: [
                [trace.get(0, last_step), trace.get(1, last_step)],
                [trace.get(4, last_step), trace.get(5, last_step)],
            ],
        }
    }

    fn options(&self) -> &ProofOptions {
        &self.options
    }

    fn new_trace_lde<E: FieldElement<BaseField = Self::BaseField>>(
        &self,
        trace_info: &TraceInfo,
        main_trace: &ColMatrix<Self::BaseField>,
        domain: &StarkDomain<Self::BaseField>,
    ) -> (Self::TraceLde<E>, TracePolyTable<E>) {
        DefaultTraceLde::new(trace_info, main_trace, domain)
    }

    fn new_evaluator<'a, E: FieldElement<BaseField = Self::BaseField>>(
        &self,
        air: &'a Self::Air,
        aux_rand_elements: Option<Self::AuxRandElements<E>>,
        composition_coefficients: ConstraintCompositionCoefficients<E>,
    ) -> Self::ConstraintEvaluator<'a, E> {
        DefaultConstraintEvaluator::new(air, aux_rand_elements, composition_coefficients)
    }
}
