// Copyright (c) Facebook, Inc. and its affiliates.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use alloc::vec::Vec;
use crypto::ElementHasher;
use math::FieldElement;
use utils::{
    ByteReader, ByteWriter, Deserializable, DeserializationError, Serializable, SliceReader,
};

use crate::{EvaluationFrame, LagrangeKernelEvaluationFrame};

// OUT-OF-DOMAIN FRAME
// ================================================================================================

/// Trace and constraint polynomial evaluations at an out-of-domain point.
///
/// This struct contains the following evaluations:
/// * Evaluations of all trace polynomials at *z*.
/// * Evaluations of all trace polynomials at *z * g*.
/// * Evaluations of Lagrange kernel trace polynomial (if any) at *z*, *z * g*, *z * g^2*, ...,
///   *z * g^(2^(v-1))*, where `v == log(trace_len)`
/// * Evaluations of constraint composition column polynomials at *z*.
///
/// where *z* is an out-of-domain point and *g* is the generator of the trace domain.
///
/// Internally, the evaluations are stored as a sequence of bytes. Thus, to retrieve the
/// evaluations, [parse()](OodFrame::parse) function should be used.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OodFrame {
    trace_states: Vec<u8>,
    lagrange_kernel_trace_states: Vec<u8>,
    evaluations: Vec<u8>,
}

impl OodFrame {
    // UPDATERS
    // --------------------------------------------------------------------------------------------

    /// Updates the trace state portion of this out-of-domain frame. This also returns a compacted
    /// version of the out-of-domain frame (including the Lagrange kernel frame, if any) with the
    /// rows interleaved. This is done so that reseeding of the random coin needs to be done only
    /// once as opposed to once per each row.
    ///
    /// # Panics
    /// Panics if evaluation frame has already been set.
    pub fn set_trace_states<E: FieldElement>(
        &mut self,
        trace_states: &OodFrameTraceStates<E>,
    ) -> Vec<E> {
        assert!(self.trace_states.is_empty(), "trace sates have already been set");

        // save the evaluations with the current and next evaluations interleaved for each polynomial

        let mut result = vec![];
        for col in 0..trace_states.num_columns() {
            result.push(trace_states.current_row[col]);
            result.push(trace_states.next_row[col]);
        }

        // there are 2 frames: current and next
        let frame_size: u8 = 2;

        self.trace_states.write_u8(frame_size);
        self.trace_states.write_many(&result);

        // save the Lagrange kernel evaluation frame (if any)
        let lagrange_trace_states = {
            let lagrange_trace_states = match trace_states.lagrange_kernel_frame {
                Some(ref lagrange_trace_states) => lagrange_trace_states.inner().to_vec(),
                None => Vec::new(),
            };

            // trace states length will be smaller than u8::MAX, since it is `== log2(trace_len) + 1`
            debug_assert!(lagrange_trace_states.len() < u8::MAX.into());
            self.lagrange_kernel_trace_states.write_u8(lagrange_trace_states.len() as u8);
            self.lagrange_kernel_trace_states.write_many(&lagrange_trace_states);

            lagrange_trace_states
        };

        result.into_iter().chain(lagrange_trace_states).collect()
    }

    /// Updates constraint evaluation portion of this out-of-domain frame.
    ///
    /// # Panics
    /// Panics if:
    /// * Constraint evaluations have already been set.
    /// * `evaluations` is an empty vector.
    pub fn set_constraint_evaluations<E: FieldElement>(&mut self, evaluations: &[E]) {
        assert!(self.evaluations.is_empty(), "constraint evaluations have already been set");
        assert!(!evaluations.is_empty(), "cannot set to empty constraint evaluations");
        self.evaluations.write_many(evaluations);
    }

    // PARSER
    // --------------------------------------------------------------------------------------------
    /// Returns an out-of-domain trace frame and a vector of out-of-domain constraint evaluations
    /// contained in `self`.
    ///
    /// # Panics
    /// Panics if either `main_trace_width` or `num_evaluations` are equal to zero.
    ///
    /// # Errors
    /// Returns an error if:
    /// * Valid [`crate::EvaluationFrame`]s for the specified `main_trace_width` and
    ///   `aux_trace_width` could not be parsed from the internal bytes.
    /// * A vector of evaluations specified by `num_evaluations` could not be parsed from the
    ///   internal bytes.
    /// * Any unconsumed bytes remained after the parsing was complete.
    pub fn parse<E: FieldElement>(
        self,
        main_trace_width: usize,
        aux_trace_width: usize,
        num_evaluations: usize,
    ) -> Result<(OodFrameTraceStates<E>, Vec<E>), DeserializationError> {
        assert!(main_trace_width > 0, "trace width cannot be zero");
        assert!(num_evaluations > 0, "number of evaluations cannot be zero");

        // parse main and auxiliary trace evaluation frames
        let (current_row, next_row) = {
            let mut reader = SliceReader::new(&self.trace_states);
            let frame_size = reader.read_u8()? as usize;
            let mut trace = reader.read_many((main_trace_width + aux_trace_width) * frame_size)?;

            if reader.has_more_bytes() {
                return Err(DeserializationError::UnconsumedBytes);
            }

            let mut current_row = Vec::with_capacity(main_trace_width);
            let mut next_row = Vec::with_capacity(main_trace_width);

            while !trace.is_empty() {
                current_row.push(trace.pop().expect(""));
                next_row.push(trace.pop().expect(""));
            }

            (current_row, next_row)
        };

        // parse Lagrange kernel column trace
        let mut reader = SliceReader::new(&self.lagrange_kernel_trace_states);
        let lagrange_kernel_frame_size = reader.read_u8()? as usize;
        let lagrange_kernel_frame = if lagrange_kernel_frame_size > 0 {
            let lagrange_kernel_trace = reader.read_many(lagrange_kernel_frame_size)?;

            Some(LagrangeKernelEvaluationFrame::new(lagrange_kernel_trace))
        } else {
            None
        };

        // parse the constraint evaluations
        let mut reader = SliceReader::new(&self.evaluations);
        let evaluations = reader.read_many(num_evaluations)?;
        if reader.has_more_bytes() {
            return Err(DeserializationError::UnconsumedBytes);
        }

        Ok((
            OodFrameTraceStates::new(
                current_row,
                next_row,
                main_trace_width,
                lagrange_kernel_frame,
            ),
            evaluations,
        ))
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for OodFrame {
    /// Serializes `self` and writes the resulting bytes into the `target`.
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        // write trace rows
        target.write_u16(self.trace_states.len() as u16);
        target.write_bytes(&self.trace_states);

        // write Lagrange kernel column trace rows
        target.write_u16(self.lagrange_kernel_trace_states.len() as u16);
        target.write_bytes(&self.lagrange_kernel_trace_states);

        // write constraint evaluations row
        target.write_u16(self.evaluations.len() as u16);
        target.write_bytes(&self.evaluations)
    }

    /// Returns an estimate of how many bytes are needed to represent self.
    fn get_size_hint(&self) -> usize {
        self.trace_states.len() + self.evaluations.len() + 4
    }
}

impl Deserializable for OodFrame {
    /// Reads a OOD frame from the specified `source` and returns the result
    ///
    /// # Errors
    /// Returns an error of a valid OOD frame could not be read from the specified `source`.
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        // read trace rows
        let num_trace_state_bytes = source.read_u16()? as usize;
        let trace_states = source.read_vec(num_trace_state_bytes)?;

        // read Lagrange kernel column trace rows
        let num_lagrange_state_bytes = source.read_u16()? as usize;
        let lagrange_kernel_trace_states = source.read_vec(num_lagrange_state_bytes)?;

        // read constraint evaluations row
        let num_constraint_evaluation_bytes = source.read_u16()? as usize;
        let evaluations = source.read_vec(num_constraint_evaluation_bytes)?;

        Ok(OodFrame {
            trace_states,
            lagrange_kernel_trace_states,
            evaluations,
        })
    }
}

// OOD FRAME TRACE STATES
// ================================================================================================

/// Stores the trace evaluations at `z` and `gz`, where `z` is a random Field element. If
/// the Air contains a Lagrange kernel auxiliary column, then that column interpolated polynomial
/// will be evaluated at `z`, `gz`, `g^2 z`, ... `g^(2^(v-1)) z`, where `v == log(trace_len)`, and
/// stored in `lagrange_kernel_frame`.
pub struct OodFrameTraceStates<E: FieldElement> {
    current_row: Vec<E>,
    next_row: Vec<E>,
    main_trace_width: usize,
    lagrange_kernel_frame: Option<LagrangeKernelEvaluationFrame<E>>,
}

impl<E: FieldElement> OodFrameTraceStates<E> {
    /// Creates a new [`OodFrameTraceStates`] from current, next and optionally Lagrange kernel frames.
    pub fn new(
        current_row: Vec<E>,
        next_row: Vec<E>,
        main_trace_width: usize,
        lagrange_kernel_frame: Option<LagrangeKernelEvaluationFrame<E>>,
    ) -> Self {
        assert_eq!(current_row.len(), next_row.len());

        Self {
            current_row,
            next_row,
            main_trace_width,
            lagrange_kernel_frame,
        }
    }

    /// Returns the number of columns for the current and next frames.
    pub fn num_columns(&self) -> usize {
        self.current_row.len()
    }

    /// Returns the current row, consisting of both main and auxiliary columns.
    pub fn current_row(&self) -> &[E] {
        &self.current_row
    }

    /// Returns the next frame, consisting of both main and auxiliary columns.
    pub fn next_row(&self) -> &[E] {
        &self.next_row
    }

    pub fn main_frame(&self) -> EvaluationFrame<E> {
        let current = self.current_row[0..self.main_trace_width].to_vec();
        let next = self.next_row[0..self.main_trace_width].to_vec();

        EvaluationFrame::from_rows(current, next)
    }

    pub fn aux_frame(&self) -> Option<EvaluationFrame<E>> {
        if self.has_aux_frame() {
            let current = self.current_row[self.main_trace_width..].to_vec();
            let next = self.next_row[self.main_trace_width..].to_vec();

            Some(EvaluationFrame::from_rows(current, next))
        } else {
            None
        }
    }

    pub fn hash<H: ElementHasher<BaseField = E::BaseField>>(&self) -> H::Digest {
        // TODO: This has to be the interleaved strategy to correspond with
        // `OodFrame::set_trace_states()`
        todo!()
    }

    /// Returns the Lagrange kernel frame, if any.
    pub fn lagrange_kernel_frame(&self) -> Option<&LagrangeKernelEvaluationFrame<E>> {
        self.lagrange_kernel_frame.as_ref()
    }

    fn has_aux_frame(&self) -> bool {
        self.current_row.len() > self.main_trace_width
    }
}
