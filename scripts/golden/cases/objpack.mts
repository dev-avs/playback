// golden/objpack.json: src/vendor/objpack.ts (SLObjPack) pack/unpack over varied values, the real spicy payloads, and malformed packs.
import type { Fixtures } from "../run.mts";
import { SLObjPack } from "../../../src/vendor/objpack.ts";

const attempt = (f: () => unknown) => {
  try {
    return { ok: f() };
  } catch (e) {
    return { error: (e as Error).message };
  }
};

// JSON text, so that keys such as __proto__ are own properties as the wire would give them
const parse = (s: string) => JSON.parse(s);

const values: [string, unknown][] = [
  ["null", null],
  ["true", true],
  ["false", false],
  ["zero", 0],
  ["int", 42],
  ["negative", -7],
  ["float", 0.1],
  ["float-sum", 0.1 + 0.2],
  ["big", 9007199254740993],
  ["exp", 1e21],
  ["small", 5e-324],
  ["max", 1.7976931348623157e308],
  ["neg-float", -1.5e-7],
  ["string", "hello"],
  ["empty-string", ""],
  ["unicode", "日本語 가사 🎵 é"],
  ["numeric-string", "1"],
  ["array-empty", []],
  ["array-one", ["x"]],
  ["array-one-nested", [[1]]],
  ["array-mixed", [1, "a", null, true, [2, 3], { a: 1 }]],
  ["array-repeat", ["a", "a", "b", "a", "b", "c"]],
  ["object-empty", {}],
  ["object", { a: 1, b: "two", c: null, d: [1, 2], e: { f: true } }],
  ["object-key-equals-value", { a: "a", b: "a" }],
  ["number-vs-string", { a: 1, b: "1", c: true, d: "true", e: null, f: "null" }],
  ["integer-keys", parse('{"b":1,"2":"x","1":"y","a":2,"10":3,"01":4,"4294967295":5,"4294967294":6}')],
  ["schema-array", [{ a: 1, b: 2 }, { a: 3, b: 4 }, { a: 5, b: 6 }]],
  ["schema-array-two", [{ a: 1 }, { a: 2 }]],
  ["schema-key-order-differs", [{ a: 1, b: 2 }, { b: 3, a: 4 }]],
  ["schema-key-count-differs", [{ a: 1, b: 2 }, { a: 3 }]],
  ["schema-with-empty-first", [{}, {}]],
  ["schema-then-array", [{ a: 1 }, [1]]],
  ["schema-nested", [{ k: [{ x: 1, y: 2 }, { x: 3, y: 4 }], n: { q: 1 } }, { k: [], n: {} }]],
  ["schema-integer-keys", [parse('{"b":1,"1":2}'), parse('{"1":3,"b":4}')]],
  ["frequency-ties", ["b", "a", "b", "a", "c", "d"]],
  ["frequency-keys-and-values", { x: "x", y: ["x", "y", "y"] }],
  ["deep", parse("[".repeat(100) + "1" + "]".repeat(100))],
  ["long-array", Array.from({ length: 300 }, (_, i) => i % 7)],
  ["long-schema", Array.from({ length: 200 }, (_, i) => ({ t: i * 0.25, w: `w${i % 5}`, p: i % 2 === 0 }))],
  ["forbidden-proto", parse('{"__proto__":1}')],
  ["forbidden-constructor", { constructor: 1 }],
  ["forbidden-prototype-nested", { a: [{ prototype: 1 }] }],
  ["proto-as-value", { a: "__proto__" }],
];

const limitCases: [string, unknown, unknown][] = [
  ["depth-ok", { depth: 3 }, [[[1]]]],
  ["depth-over", { depth: 2 }, [[[1]]]],
  ["depth-zero", { depth: 0 }, [1]],
  ["array-length", { arrayLength: 2 }, [1, 2, 3]],
  ["array-length-ok", { arrayLength: 3 }, [1, 2, 3]],
  ["object-keys", { objectKeys: 1 }, { a: 1, b: 2 }],
  ["forbidden-custom", null, { x: 1 }],
];

export default (fx: Fixtures) => {
  const packer = new SLObjPack();
  const out: any = { pack: [], roundTrip: [], real: [], limits: [], unpack: [] };

  for (const [name, input] of values) {
    const packed = attempt(() => packer.pack(input));
    out.pack.push({ name, in: input, ...packed });
    if ("ok" in packed) out.roundTrip.push({ name, packed: packed.ok, ...attempt(() => packer.unpack(packed.ok)) });
  }

  for (const f of fx.list("spicy").concat(fx.list("spicy-edge"))) {
    if (!f.endsWith(".json")) continue;
    const data = fx.json(f);
    const packed = packer.pack(data);
    out.real.push({ file: f, packed, unpacked: packer.unpack(packed) });
  }

  for (const [name, limits, input] of limitCases) {
    const options = name === "forbidden-custom" ? { forbiddenKeys: ["x"] } : { limits };
    const p = new SLObjPack(options as any);
    const packed = attempt(() => p.pack(input));
    out.limits.push({ name, options, in: input, ...packed });
    // the same limits on the way back, from a default-packed stream
    const plain = packer.pack(input);
    out.limits.push({ name: name + " (unpack)", options, packed: plain, ...attempt(() => p.unpack(plain)) });
  }

  // hand-made packs: [valuesList, stream]
  const unpack: [string, unknown, unknown?][] = [
    ["not-array", "x"],
    ["null", null],
    ["object", {}],
    ["length-1", [[]]],
    ["length-3", [[], [], []]],
    ["values-not-array", [{}, []]],
    ["stream-not-array", [[], "s"]],
    ["empty-stream", [[], []]],
    ["primitive", [["hi"], [0]]],
    ["pointer-negative-zero", [["hi"], [-0]]],
    ["pointer-out-of-range", [["hi"], [1]]],
    ["pointer-fraction", [["hi"], [0.5]]],
    ["pointer-integral-float", [["hi", "yo"], [1.0]]],
    ["pointer-string", [["hi"], ["0"]]],
    ["pointer-null", [["hi"], [null]]],
    ["pointer-huge", [["hi"], [1e300]]],
    ["pointer-tiny", [["hi"], [1e-7]]],
    ["pointer-array", [["hi"], [[1, 2]]]],
    ["pointer-object", [["hi"], [{}]]],
    ["opcode-unknown", [[], [-7]]],
    ["opcode-unknown-big", [[], [-1e21]]],
    ["empty-array", [[], [-4]]],
    ["empty-object", [[], [-6]]],
    ["single", [["a"], [-5, 0]]],
    ["single-missing", [["a"], [-5]]],
    ["object-op", [["k", "v"], [-1, 1, 0, 1]]],
    ["object-dup-keys", [["k", 1, 2], [-1, 2, 0, 0, 1, 2]]],
    ["object-integer-keys", [["b", "2", "1", 1, 2, 3], [-1, 3, 0, 1, 2, 3, 4, 5]]],
    ["object-key-count-bad", [["k"], [-1, -1]]],
    ["object-key-count-fraction", [["k"], [-1, 1.5]]],
    ["object-key-count-string", [["k"], [-1, "1"]]],
    ["object-key-count-too-big", [["k"], [-1, 99999]]],
    ["object-not-enough-stream", [["k", "v"], [-1, 2, 0]]],
    ["object-key-number", [["k", 1], [-1, 1, 1, 0]]],
    ["object-key-null", [[null, 1], [-1, 1, 0, 1]]],
    ["object-key-bool", [[true, 1], [-1, 1, 0, 1]]],
    ["object-key-forbidden", [["__proto__", 1], [-1, 1, 0, 1]]],
    ["object-key-constructor", [["constructor", 1], [-1, 1, 0, 1]]],
    ["array-op", [["a", "b"], [-2, 2, 0, 1]]],
    ["array-short", [["a"], [-2, 3, 0, 0]]],
    ["array-count-negative", [["a"], [-2, -1]]],
    ["array-nested", [["a"], [-2, 2, -4, -5, 0]]],
    ["schema-op", [["a", "b", 1, 2, 3, 4], [-3, 2, 2, 0, 1, 2, 3, 4, 5]]],
    ["schema-zero-keys", [[], [-3, 3, 0]]],
    ["schema-zero-items", [["a"], [-3, 0, 1, 0]]],
    ["schema-short", [["a", 1], [-3, 2, 1, 0, 1]]],
    ["schema-dup-keys", [["a", 1, 2], [-3, 1, 2, 0, 0, 1, 2]]],
    ["schema-integer-keys", [["b", "1", 1, 2], [-3, 1, 2, 0, 1, 2, 3]]],
    ["schema-budget", [["a"], [-3, 1, 1, 0, 0]], { limits: { decodeOps: 0 } }],
    ["trailing", [["a"], [0, 0]]],
    ["values-object", [[{}], [0]]],
    ["values-array", [[[]], [0]]],
    ["values-mixed", [[null, true, false, 1.5, "s"], [-2, 5, 0, 1, 2, 3, 4]]],
    ["limit-values", [["a", "b"], [0]], { limits: { valuesLength: 1 } }],
    ["limit-stream", [["a"], [-2, 2, 0, 0]], { limits: { streamLength: 3 } }],
    ["limit-depth", [["a"], [-5, -5, -5, 0]], { limits: { depth: 2 } }],
    ["limit-depth-ok", [["a"], [-5, -5, 0]], { limits: { depth: 2 } }],
    ["limit-array-items", [["a"], [-2, 2, 0, 0]], { limits: { arrayLength: 1 } }],
    ["limit-object-keys", [["a", "b", 1, 2], [-1, 2, 0, 1, 2, 3]], { limits: { objectKeys: 1 } }],
    ["limit-schema-keys", [["a", "b", 1, 2], [-3, 1, 2, 0, 1, 2, 3]], { limits: { objectKeys: 1 } }],
    ["forbidden-custom", [["x", 1], [-1, 1, 0, 1]], { forbiddenKeys: ["x"] }],
    ["forbidden-default-off", [["__proto__", 1], [-1, 1, 0, 1]], { forbiddenKeys: [] }],
  ];
  for (const [name, input, options] of unpack) {
    const p = new SLObjPack(options as any);
    out.unpack.push({ name, options: options ?? null, in: input, ...attempt(() => p.unpack(input)) });
  }
  return out;
};
