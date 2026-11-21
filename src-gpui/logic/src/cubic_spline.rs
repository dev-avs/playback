//! Port of the npm package `cubic-spline` (node_modules/cubic-spline/index.js,
//! `NaturalSpline` in src/lyrics/lyricAnimator.ts): a natural cubic spline.
//!
//! Design note: translated function for function, same order, same float
//! expressions (so results are bit-identical). `Float64Array`/`Array` become
//! `Vec<f64>`. JavaScript reads past the end of an array as `undefined`, which
//! turns the arithmetic into NaN (e.g. `at(x)` for x beyond the last knot);
//! `get` reproduces that by returning NaN for an out-of-range index instead of
//! panicking.

#![allow(clippy::needless_range_loop)] // index loops kept as in the JS

/// An index read the way JavaScript does it: out of range is `undefined`, which
/// every arithmetic expression here turns into NaN.
fn get(v: &[f64], i: usize) -> f64 {
    v.get(i).copied().unwrap_or(f64::NAN)
}

/// index.js:1 (`module.exports = class Spline`)
#[derive(Debug, Clone)]
pub struct NaturalSpline {
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
    pub ks: Vec<f64>,
}

impl NaturalSpline {
    /// index.js:2-6
    pub fn new(xs: Vec<f64>, ys: Vec<f64>) -> NaturalSpline {
        let mut s = NaturalSpline { xs, ys, ks: Vec::new() };
        let len = s.xs.len();
        s.ks = s.get_natural_ks(vec![0.0; len]);
        s
    }

    /// index.js:8-45
    pub fn get_natural_ks(&self, ks: Vec<f64>) -> Vec<f64> {
        let xs = &self.xs;
        let ys = &self.ys;
        let n = xs.len() - 1;
        let mut a = zeros_mat(n + 1, n + 2);

        for i in 1..n {
            // rows
            a[i][i - 1] = 1.0 / (xs[i] - xs[i - 1]);
            a[i][i] = 2.0 * (1.0 / (xs[i] - xs[i - 1]) + 1.0 / (xs[i + 1] - xs[i]));
            a[i][i + 1] = 1.0 / (xs[i + 1] - xs[i]);
            a[i][n + 1] = 3.0
                * ((ys[i] - ys[i - 1]) / ((xs[i] - xs[i - 1]) * (xs[i] - xs[i - 1]))
                    + (ys[i + 1] - ys[i]) / ((xs[i + 1] - xs[i]) * (xs[i + 1] - xs[i])));
        }

        a[0][0] = 2.0 / (xs[1] - xs[0]);
        a[0][1] = 1.0 / (xs[1] - xs[0]);
        a[0][n + 1] = (3.0 * (ys[1] - ys[0])) / ((xs[1] - xs[0]) * (xs[1] - xs[0]));

        a[n][n - 1] = 1.0 / (xs[n] - xs[n - 1]);
        a[n][n] = 2.0 / (xs[n] - xs[n - 1]);
        a[n][n + 1] = (3.0 * (ys[n] - ys[n - 1])) / ((xs[n] - xs[n - 1]) * (xs[n] - xs[n - 1]));

        solve(a, ks)
    }

    /// index.js:47-65 (inspired by https://stackoverflow.com/a/40850313/4417327)
    pub fn get_index_before(&self, target: f64) -> usize {
        let mut low = 0usize;
        let mut high = self.xs.len();
        let mut mid;
        while low < high {
            mid = (low + high) / 2;
            if self.xs[mid] < target && mid != low {
                low = mid;
            } else if self.xs[mid] >= target && mid != high {
                high = mid;
            } else {
                high = low;
            }
        }
        low + 1
    }

    /// index.js:67-82
    pub fn at(&self, x: f64) -> f64 {
        let i = self.get_index_before(x);
        let (xs, ys, ks) = (&self.xs, &self.ys, &self.ks);
        let t = (x - get(xs, i - 1)) / (get(xs, i) - get(xs, i - 1));
        let a = get(ks, i - 1) * (get(xs, i) - get(xs, i - 1)) - (get(ys, i) - get(ys, i - 1));
        let b = -get(ks, i) * (get(xs, i) - get(xs, i - 1)) + (get(ys, i) - get(ys, i - 1));
        (1.0 - t) * get(ys, i - 1) + t * get(ys, i) + t * (1.0 - t) * (a * (1.0 - t) + b * t)
    }
}

/// index.js:85-129
fn solve(mut a: Vec<Vec<f64>>, mut ks: Vec<f64>) -> Vec<f64> {
    let m = a.len();
    let mut h = 0usize;
    let mut k = 0usize;
    while h < m && k <= m {
        let mut i_max = 0usize;
        let mut max = f64::NEG_INFINITY;
        for i in h..m {
            let v = a[i][k].abs();
            if v > max {
                i_max = i;
                max = v;
            }
        }

        if a[i_max][k] == 0.0 {
            k += 1;
        } else {
            swap_rows(&mut a, h, i_max);
            for i in (h + 1)..m {
                let f = a[i][k] / a[h][k];
                a[i][k] = 0.0;
                for j in (k + 1)..=m {
                    a[i][j] -= a[h][j] * f;
                }
            }
            h += 1;
            k += 1;
        }
    }

    // rows = columns
    for i in (0..m).rev() {
        let mut v = 0.0;
        // JS truthiness: neither 0 nor NaN
        if a[i][i] != 0.0 && !a[i][i].is_nan() {
            v = a[i][m] / a[i][i];
        }
        ks[i] = v;
        for j in (0..i).rev() {
            // rows
            a[j][m] -= a[j][i] * v;
            a[j][i] = 0.0;
        }
    }
    ks
}

/// index.js:131-135
fn zeros_mat(r: usize, c: usize) -> Vec<Vec<f64>> {
    (0..r).map(|_| vec![0.0; c]).collect()
}

/// index.js:137-141
fn swap_rows(m: &mut [Vec<f64>], k: usize, l: usize) {
    m.swap(k, l);
}
