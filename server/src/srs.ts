// A thin wrapper over crates/srs compiled to wasm (see crates/srs/src/wasm.rs for the ABI), so the server derives
// exactly the schedules the clients do.

export type State = "new" | "learning" | "review" | "relearning";

export interface Schedule {
  readonly state: State;
  readonly due: number;
  readonly stability: number;
  readonly difficulty: number;
  readonly reps: number;
  readonly lapses: number;
  readonly last_review: number | null;
}

export interface FoldReview {
  readonly reviewed_at: number;
  readonly rating: number;
}

export interface Scheduler {
  /** Folds reviews, already ordered by `(reviewed_at, id)`, over a card created at `createdAt`. */
  readonly fold: (createdAt: number, reviews: ReadonlyArray<FoldReview>) => Schedule;
}

interface Exports {
  readonly memory: WebAssembly.Memory;
  readonly srs_alloc: (len: number) => number;
  readonly srs_free: (ptr: number, len: number) => void;
  readonly srs_fold: (createdAt: number, reviews: number, count: number, out: number) => number;
}

const STATES: ReadonlyArray<State> = ["new", "learning", "review", "relearning"];

export const makeScheduler = (module: WebAssembly.Module): Scheduler => {
  const exports = new WebAssembly.Instance(module, {}).exports as unknown as Exports;
  return {
    fold: (createdAt, reviews) => {
      const inputLen = Math.max(reviews.length * 2, 1);
      const input = exports.srs_alloc(inputLen);
      const output = exports.srs_alloc(7);
      try {
        // Views are taken after both allocations, which may grow (and detach) the memory.
        const pairs = new Float64Array(exports.memory.buffer, input, inputLen);
        reviews.forEach((review, i) => {
          pairs[i * 2] = review.reviewed_at;
          pairs[i * 2 + 1] = review.rating;
        });
        if (exports.srs_fold(createdAt, input, reviews.length, output) !== 0) {
          throw new Error("srs_fold: invalid rating");
        }
        const out = new Float64Array(exports.memory.buffer, output, 7);
        return {
          state: STATES[out[0]!]!,
          due: out[1]!,
          stability: out[2]!,
          difficulty: out[3]!,
          reps: out[4]!,
          lapses: out[5]!,
          last_review: out[6]! < 0 ? null : out[6]!,
        };
      } finally {
        exports.srs_free(input, inputLen);
        exports.srs_free(output, 7);
      }
    },
  };
};
