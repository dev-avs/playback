// Japanese readings for kanji, run in its own utility process.
//
// Kanji has no algorithmic reading, so romanizing it needs a dictionary:
// kuromoji (IPADIC) splits each line into words and gives every word its
// reading in context (女の子 → オンナノコ). The dictionary costs ~300 MB once
// loaded, so it lives in a throwaway process that exits after answering, not
// in the main process for the rest of the session.
//
// In: { texts: string[] }. Out: { tokens: { s, r?, p, d, b }[][] } — surface,
// reading, part of speech, its first detail, base form — one token list per
// text, in order, covering every character of it. The part of speech decides
// where the romaji gets spaces (src/romanize.ts, attaches).

const path = require("path");
const kuromoji = require("kuromoji");

process.parentPort.once("message", ({ data }) => {
  const dicPath = path.join(path.dirname(require.resolve("kuromoji/package.json")), "dict");
  kuromoji.builder({ dicPath }).build((err, tokenizer) => {
    if (err) {
      process.parentPort.postMessage({ error: String(err) });
      return;
    }
    const tokens = data.texts.map((text) =>
      tokenizer.tokenize(text).map((t) => ({
        s: t.surface_form,
        ...(t.reading ? { r: t.reading } : {}),
        p: t.pos,
        d: t.pos_detail_1,
        b: t.basic_form,
      })),
    );
    process.parentPort.postMessage({ tokens });
  });
});
