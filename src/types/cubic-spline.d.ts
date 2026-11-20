// The `cubic-spline` package ships no types.
declare module "cubic-spline" {
  export default class Spline {
    constructor(xs: number[], ys: number[]);
    at(x: number): number;
  }
}
