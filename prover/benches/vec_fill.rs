// Copyright (c) Facebook, Inc. and its affiliates.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Benchmarks comparing different methods of filling a Vec<u64> with 1 million sequential elements.
//!
//! This benchmark compares 4 different approaches:
//! 1. Using `Vec::with_capacity(1_000_000)` followed by `Vec::push()` in a loop
//! 2. Using `uninit_vector(1_000_000)` with index assignments `vec[idx] = ...`
//! 3. Using a local copy of `uninit_vector2` with `MaybeUninit::write()`
//! 4. Using `vec![MaybeUninit::<T>::uninit(); 1_000_000]` with `MaybeUninit::write()`

use std::{hint::black_box, mem::MaybeUninit};

use criterion::{criterion_group, criterion_main, Criterion};
use utils::uninit_vector;

// CONSTANTS
// ================================================================================================

const VEC_SIZE: usize = 10_000_000;

// HELPER FUNCTIONS
// ================================================================================================

/// Local copy of uninit_vector2 since it might not be accessible in this crate
unsafe fn uninit_vector2<T>(length: usize) -> Vec<MaybeUninit<T>> {
    let mut vector = Vec::with_capacity(length);
    vector.set_len(length);
    vector
}

// BENCHMARK FUNCTIONS
// ================================================================================================

fn fill_with_capacity_and_push(c: &mut Criterion) {
    c.bench_function(
        &format!("vec_fill/with_capacity_push/{}", VEC_SIZE),
        |bench| {
            bench.iter(|| {
                let mut vec = Vec::with_capacity(VEC_SIZE);
                for i in 0..VEC_SIZE {
                    vec.push((i + 1) as u64);
                }
                vec
            });
        },
    );
}

fn fill_with_capacity_and_push_wrong_bound(c: &mut Criterion) {
    c.bench_function(
        &format!("vec_fill/with_capacity_push_wrong_bound/{}", VEC_SIZE),
        |bench| {
            bench.iter(|| {
                let mut vec = Vec::with_capacity(VEC_SIZE);
                for i in 1..=VEC_SIZE {
                    vec.push(i  as u64);
                }
                vec
            });
        },
    );
}

fn fill_with_uninit_vector_index(c: &mut Criterion) {
    c.bench_function(
        &format!("vec_fill/uninit_vector/{}", VEC_SIZE),
        |bench| {
            bench.iter(|| {
                let mut vec: Vec<u64> = unsafe { uninit_vector(VEC_SIZE) };
                for i in 0..VEC_SIZE {
                    vec[i] = (i + 1) as u64;
                }
                vec
            });
        },
    );
}

fn fill_with_uninit_vector2_write(c: &mut Criterion) {
    c.bench_function(
        &format!("vec_fill/uninit_vector2/{}", VEC_SIZE),
        |bench| {
            bench.iter(|| {
                let mut vec: Vec<MaybeUninit<u64>> = unsafe { uninit_vector2(VEC_SIZE) };
                for i in 0..VEC_SIZE {
                    vec[i].write((i + 1) as u64);
                }
                // Safety: we've initialized all elements
                black_box(unsafe { std::mem::transmute::<Vec<MaybeUninit<u64>>, Vec<u64>>(vec) })
            });
        },
    );
}

fn fill_with_box_uninit_slice(c: &mut Criterion) {
    c.bench_function(
        &format!("vec_fill/box_uninit_slice/{}", VEC_SIZE),
        |bench| {
            bench.iter(|| {
                let mut boxy = Box::new_uninit_slice(VEC_SIZE);
                for i in 0..VEC_SIZE {
                    boxy[i].write((i + 1) as u64);
                }
                // Safety: we've initialized all elements
                let boxy = unsafe { boxy.assume_init() };
                black_box(Vec::from(boxy));
            });
        },
    );
}

fn fill_with_maybe_uninit_vec_macro(c: &mut Criterion) {
    c.bench_function(
        &format!("vec_fill/maybe_uninit_vec_macro/{}", VEC_SIZE),
        |bench| {
            bench.iter(|| {
                let mut vec: Vec<MaybeUninit<u64>> = vec![MaybeUninit::<u64>::uninit(); VEC_SIZE];
                for i in 0..VEC_SIZE {
                    vec[i].write((i + 1) as u64);
                }
                // Safety: we've initialized all elements
                black_box(unsafe { std::mem::transmute::<Vec<MaybeUninit<u64>>, Vec<u64>>(vec) })
            });
        },
    );
}

criterion_group!(
    benches,
    fill_with_capacity_and_push,
    fill_with_capacity_and_push_wrong_bound,
    fill_with_uninit_vector_index,
    fill_with_uninit_vector2_write,
    fill_with_box_uninit_slice,
    fill_with_maybe_uninit_vec_macro
);
criterion_main!(benches);
