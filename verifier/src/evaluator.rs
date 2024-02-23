// Copyright (c) Facebook, Inc. and its affiliates.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use air::{Air, AuxTraceRandElements, ConstraintCompositionCoefficients, EvaluationFrame};
use math::{polynom, FieldElement};
use utils::collections::*;

// CONSTRAINT EVALUATION
// ================================================================================================

/// Evaluates constraints for the specified evaluation frame.
pub fn evaluate_constraints<A: Air, E: FieldElement<BaseField = A::BaseField>>(
    air: &A,
    composition_coefficients: ConstraintCompositionCoefficients<E>,
    main_trace_frame: &EvaluationFrame<E>,
    aux_trace_frame: &Option<EvaluationFrame<E>>,
    lagrange_kernel_column_frame: Option<&[E]>,
    aux_rand_elements: AuxTraceRandElements<E>,
    x: E,
) -> E {
    // 1 ----- evaluate transition constraints ----------------------------------------------------

    // initialize a buffer to hold transition constraint evaluations
    let t_constraints = air.get_transition_constraints(&composition_coefficients.transition);

    // compute values of periodic columns at x
    let periodic_values = air
        .get_periodic_column_polys()
        .iter()
        .map(|poly| {
            let num_cycles = air.trace_length() / poly.len();
            let x = x.exp_vartime((num_cycles as u32).into());
            polynom::eval(poly, x)
        })
        .collect::<Vec<_>>();

    // evaluate transition constraints for the main trace segment
    let mut t_evaluations1 = E::zeroed_vector(t_constraints.num_main_constraints());
    air.evaluate_transition(main_trace_frame, &periodic_values, &mut t_evaluations1);

    // evaluate transition constraints for auxiliary trace segments (if any)
    let mut t_evaluations2 = E::zeroed_vector(t_constraints.num_aux_constraints());
    if let Some(aux_trace_frame) = aux_trace_frame {
        // `t_evaluations_2` would be empty here if there is only 1 auxiliary column, and that
        // column is the Lagrange kernel column
        if !t_evaluations2.is_empty() {
            air.evaluate_aux_transition(
                main_trace_frame,
                aux_trace_frame,
                &periodic_values,
                &aux_rand_elements,
                &mut t_evaluations2,
            );
        }
    }

    // merge all constraint evaluations into a single value by computing their random linear
    // combination using coefficients drawn from the public coin. this also divides the result
    // by the divisor of transition constraints.
    let mut result = t_constraints.combine_evaluations::<E>(&t_evaluations1, &t_evaluations2, x);

    // 2 ----- evaluate Lagrange kernel transition constraints ------------------------------------

    if let Some(lagrange_kernel_column_frame) = lagrange_kernel_column_frame {
        let mut lagrange_t_evaluations = E::zeroed_vector(lagrange_kernel_column_frame.len());
        air.evaluate_lagrange_kernel_aux_transition(
            lagrange_kernel_column_frame,
            &aux_rand_elements,
            &mut lagrange_t_evaluations,
        );

        let lagrange_t_constraints = air.get_lagrange_kernel_transition_constraints(
            composition_coefficients.lagrange_kernel_transition,
        );

        result += lagrange_t_constraints.combine_evaluations::<E>(&lagrange_t_evaluations, x);
    }

    // 3 ----- evaluate boundary constraints ------------------------------------------------------

    // get boundary constraints grouped by common divisor from the AIR
    let b_constraints = air.get_boundary_constraints(
        &aux_rand_elements,
        &composition_coefficients.boundary,
        composition_coefficients.lagrange_kernel_boundary,
    );

    // iterate over boundary constraint groups for the main trace segment (each group has a
    // distinct divisor), evaluate constraints in each group and add their combination to the
    // result
    for group in b_constraints.main_constraints().iter() {
        result += group.evaluate_at(main_trace_frame.current(), x);
    }

    // iterate over boundary constraint groups for auxiliary trace segments (each group has a
    // distinct divisor), evaluate constraints in each group and add their combination to the
    // result
    if let Some(aux_trace_frame) = aux_trace_frame {
        for group in b_constraints.aux_constraints().iter() {
            result += group.evaluate_at(aux_trace_frame.current(), x);
        }
    }

    if let Some(lagrange_kernel_column_frame) = lagrange_kernel_column_frame {
        let (constraint, divisor) = b_constraints.lagrange_kernel_constraint().expect("TODO");

        let c0 = lagrange_kernel_column_frame[0];

        // TODO: This logic is very similar to `BoundaryConstraintGroup::evaluate_at` and `DefaultConstraintEvaluator::evaluate_lagrange_kernel_constraints`
        let numerator = constraint.evaluate_at(x, c0) * *constraint.cc();
        let denominator = divisor.evaluate_at(x);

        result += numerator / denominator;
    }

    result
}
