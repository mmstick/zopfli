use std::f64;

use libc::{size_t, c_double, c_uchar};

use deflate::calculate_block_size_auto_type;
use lz77::{Lz77Store, ZopfliBlockState};
use Options;

/// Finds minimum of function `f(i)` where `i` is of type `size_t`, `f(i)` is of type
/// `c_double`, `i` is in range `start-end` (excluding `end`).
/// Returns the index to the minimum and the minimum value.
fn find_minimum(f: fn(i: size_t, context: &SplitCostContext) -> c_double, context: &SplitCostContext, start: size_t, end: size_t) -> (size_t, c_double) {
    let mut start = start;
    let mut end = end;
    if end - start < 1024 {
        let mut best = f64::MAX;
        let mut result = start;
        for i in start..end {
            let v = f(i, context);
            if v < best {
                best = v;
                result = i;
            }
        }
        (result, best)
    } else {
        /* Try to find minimum faster by recursively checking multiple points. */
        let num = 9;  /* Good value: 9. ?!?!?!?! */
        let mut p = vec![0; num];
        let mut vp = vec![0.0; num];
        let mut besti;
        let mut best;
        let mut lastbest = f64::MAX;
        let mut pos = start;

        loop {
            if end - start <= num {
                break;
            }

            for i in 0..num {
                p[i] = start + (i + 1) * ((end - start) / (num + 1));
                vp[i] = f(p[i], context);
            }

            besti = 0;
            best = vp[0];

            for (i, &item) in vp.iter().enumerate().take(num).skip(1) {
                if item < best {
                  best = item;
                  besti = i;
                }
            }
            if best > lastbest {
                break;
            }

            start = if besti == 0 { start } else { p[besti - 1] };
            end = if besti == num - 1 { end } else { p[besti + 1] };

            pos = p[besti];
            lastbest = best;
        }
        (pos, lastbest)
    }
}

/// Returns estimated cost of a block in bits.  It includes the size to encode the
/// tree and the size to encode all literal, length and distance symbols and their
/// extra bits.
///
/// litlens: lz77 lit/lengths
/// dists: ll77 distances
/// lstart: start of block
/// lend: end of block (not inclusive)
fn estimate_cost(lz77: &Lz77Store, lstart: size_t, lend: size_t) -> c_double {
    calculate_block_size_auto_type(lz77, lstart, lend)
}

/// Gets the cost which is the sum of the cost of the left and the right section
/// of the data.
fn split_cost(i: size_t, c: &SplitCostContext) -> c_double {
    estimate_cost(c.lz77, c.start, i) + estimate_cost(c.lz77, i, c.end)
}

struct SplitCostContext<'a> {
    lz77: &'a Lz77Store,
    start: size_t,
    end: size_t,
}

/// Finds next block to try to split, the largest of the available ones.
/// The largest is chosen to make sure that if only a limited amount of blocks is
/// requested, their sizes are spread evenly.
/// lz77size: the size of the LL77 data, which is the size of the done array here.
/// done: array indicating which blocks starting at that position are no longer
///     splittable (splitting them increases rather than decreases cost).
/// splitpoints: the splitpoints found so far.
/// npoints: the amount of splitpoints found so far.
/// lstart: output variable, giving start of block.
/// lend: output variable, giving end of block.
/// returns 1 if a block was found, 0 if no block found (all are done).
fn find_largest_splittable_block(lz77size: size_t, done: &[c_uchar], splitpoints: &[size_t]) -> Option<(size_t, size_t)> {
    let mut longest = 0;
    let mut found = None;

    let mut last = 0;

    for &item in splitpoints.iter() {
        if done[last] == 0 && item - last > longest {
            found = Some((last, item));
            longest = item - last;
        }
        last = item;
    }

    let end = lz77size - 1;
    if done[last] == 0 && end - last > longest {
        found = Some((last, end));
    }

    found
}

/// Prints the block split points as decimal and hex values in the terminal.
fn print_block_split_points(lz77: &Lz77Store, lz77splitpoints: &[size_t]) {
    let nlz77points = lz77splitpoints.len();
    let mut splitpoints = Vec::with_capacity(nlz77points);

    /* The input is given as lz77 indices, but we want to see the uncompressed
    index values. */
    let mut pos = 0;
    if nlz77points > 0 {
        for i in 0..lz77.size() {
            let length = if lz77.dists[i] == 0 {
                1
            } else {
                lz77.litlens[i]
            };
            if lz77splitpoints[splitpoints.len()] == i {
                splitpoints.push(pos);
                if splitpoints.len() == nlz77points {
                    break;
                }
            }
            pos += length;
        }
    }
    assert!(splitpoints.len() == nlz77points);

    println!("block split points: {} (hex: {})", splitpoints.iter().map(|&sp| format!("{}", sp)).collect::<Vec<_>>().join(" "), splitpoints.iter().map(|&sp| format!("{:x}", sp)).collect::<Vec<_>>().join(" "));
}

/// Does blocksplitting on LZ77 data.
/// The output splitpoints are indices in the LZ77 data.
/// maxblocks: set a limit to the amount of blocks. Set to 0 to mean no limit.
pub fn blocksplit_lz77(options: &Options, lz77: &Lz77Store, maxblocks: size_t, splitpoints: &mut Vec<size_t>) {

    if lz77.size() < 10 {
        return;  /* This code fails on tiny files. */
    }

    let mut llpos;
    let mut numblocks = 1;
    let mut splitcost: c_double;
    let mut origcost;
    let mut done = vec![0; lz77.size()];
    let mut lstart = 0;
    let mut lend = lz77.size();

    loop {
        if maxblocks > 0 && numblocks >= maxblocks {
          break;
        }
        let c = SplitCostContext {
            lz77: lz77,
            start: lstart,
            end: lend,
        };

        assert!(lstart < lend);
        let find_minimum_result = find_minimum(split_cost, &c, lstart + 1, lend);
        llpos = find_minimum_result.0;
        splitcost = find_minimum_result.1;

        assert!(llpos > lstart);
        assert!(llpos < lend);

        origcost = estimate_cost(lz77, lstart, lend);

        if splitcost > origcost || llpos == lstart + 1 || llpos == lend {
            done[lstart] = 1;
        } else {
            splitpoints.push(llpos);
            splitpoints.sort();
            numblocks += 1;
        }

        match find_largest_splittable_block(lz77.size(), &done, splitpoints) {
            None => break, /* No further split will probably reduce compression. */
            Some((start, end)) => {
                lstart = start;
                lend = end;
                if lend - lstart < 10 {
                    break;
                }
            }
        }
    }

    if options.verbose {
        print_block_split_points(lz77, splitpoints);
    }
}

/// Does blocksplitting on uncompressed data.
/// The output splitpoints are indices in the uncompressed bytes.
///
/// options: general program options.
/// in: uncompressed input data
/// instart: where to start splitting
/// inend: where to end splitting (not inclusive)
/// maxblocks: maximum amount of blocks to split into, or 0 for no limit
/// splitpoints: dynamic array to put the resulting split point coordinates into.
///   The coordinates are indices in the input array.
/// npoints: pointer to amount of splitpoints, for the dynamic array. The amount of
///   blocks is the amount of splitpoitns + 1.
pub fn blocksplit(options: &Options, in_data: &[u8], instart: size_t, inend: size_t, maxblocks: size_t, splitpoints: &mut Vec<size_t>) {
    let mut pos;
    let mut s = ZopfliBlockState::new(options, instart, inend, 0);

    let mut lz77splitpoints = Vec::with_capacity(maxblocks);

    let mut store = Lz77Store::new();

    splitpoints.clear();

    /* Unintuitively, Using a simple LZ77 method here instead of lz77_optimal
    results in better blocks. */
    store.greedy(&mut s, in_data, instart, inend);

    blocksplit_lz77(options, &store, maxblocks, &mut lz77splitpoints);

    let nlz77points = lz77splitpoints.len();

    /* Convert LZ77 positions to positions in the uncompressed input. */
    pos = instart;
    if nlz77points > 0 {
        for i in 0..store.size() {
            let length = if store.dists[i] == 0 { 1 } else { store.litlens[i] };
            if lz77splitpoints[splitpoints.len()] == i {
                splitpoints.push(pos);
                if splitpoints.len() == nlz77points {
                    break;
                }
            }
            pos += length as size_t;
        }
    }
    assert!(splitpoints.len() == nlz77points);
}
