//! src/vendor/objpack.ts (SLObjPack), Spicy Lyrics' binary payload codec.
//!
//! Values are `serde_json::Value` with `preserve_order`, so an object keeps the
//! key order JavaScript would give it. Two things JavaScript does on its own
//! are done here by hand: `Object.keys` lists integer-like keys ("0", "12")
//! first in ascending order (`js_keys`), and numbers are f64 whatever their
//! JSON spelling. Unreachable from a `Value`, so not ported: pass 1's checks for
//! cycles, non-finite numbers, `undefined`/bigint/symbol/function and non-plain
//! objects (a `Value` cannot hold any of them).

use serde::Deserialize;
use serde_json::{Map, Number, Value};
use std::collections::{HashMap, HashSet};
use std::fmt;

/// An `Error` thrown by the TypeScript; `.0` is its `message`.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjPackError(pub String);

impl fmt::Display for ObjPackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ObjPackError {}

fn err<T>(message: String) -> Result<T, ObjPackError> {
    Err(ObjPackError(message))
}

/// src/vendor/objpack.ts:12 `SLObjPackLimits` (JavaScript numbers, so f64)
#[derive(Clone, Debug)]
pub struct SLObjPackLimits {
    pub depth: f64,
    pub array_length: f64,
    pub object_keys: f64,
    pub stream_length: f64,
    pub values_length: f64,
    pub decode_ops: f64,
}

/// src/vendor/objpack.ts:21 `SLObjPackOptions`; `limits` fields left out are
/// `undefined`, which the constructor defaults one by one.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SLObjPackOptions {
    #[serde(default)]
    pub limits: SLObjPackLimitsOverride,
    #[serde(default)]
    pub forbidden_keys: Option<Vec<String>>,
}

/// `Partial<SLObjPackLimits>`
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SLObjPackLimitsOverride {
    pub depth: Option<f64>,
    pub array_length: Option<f64>,
    pub object_keys: Option<f64>,
    pub stream_length: Option<f64>,
    pub values_length: Option<f64>,
    pub decode_ops: Option<f64>,
}

// src/vendor/objpack.ts:30 DEFAULT_LIMITS
const DEFAULT_LIMITS: SLObjPackLimits = SLObjPackLimits {
    depth: 512.0,
    array_length: 1048576.0,   // 1 << 20
    object_keys: 65536.0,      // 1 << 16
    stream_length: 16777216.0, // 1 << 24
    values_length: 4194304.0,  // 1 << 22
    decode_ops: 4194304.0,     // 1 << 22
};

// src/vendor/objpack.ts:43 DEFAULT_FORBIDDEN_KEYS
const DEFAULT_FORBIDDEN_KEYS: [&str; 3] = ["__proto__", "constructor", "prototype"];

/// A JSONPrimitive as a `Map` key: SameValueZero, so -0 and +0 are one key
/// (hence the bits of +0), and a string key and a string value are the same.
#[derive(Clone, PartialEq, Eq, Hash)]
enum Prim {
    Null,
    Bool(bool),
    Num(u64),
    Str(String),
}

fn prim_of(node: &Value) -> Prim {
    match node {
        Value::Null => Prim::Null,
        Value::Bool(b) => Prim::Bool(*b),
        Value::Number(n) => {
            let x = n.as_f64().unwrap();
            Prim::Num(if x == 0.0 { 0.0f64.to_bits() } else { x.to_bits() })
        }
        Value::String(s) => Prim::Str(s.clone()),
        _ => unreachable!("prim_of is only called on primitives"),
    }
}

fn value_of(prim: &Prim) -> Value {
    match prim {
        Prim::Null => Value::Null,
        Prim::Bool(b) => Value::Bool(*b),
        Prim::Num(bits) => Value::Number(Number::from_f64(f64::from_bits(*bits)).unwrap()),
        Prim::Str(s) => Value::String(s.clone()),
    }
}

/// `Object.keys(map)`: integer-like keys ascending, then the rest in insertion order.
fn js_keys(map: &Map<String, Value>) -> Vec<&String> {
    let mut indexes: Vec<(u32, &String)> = Vec::new();
    let mut rest: Vec<&String> = Vec::new();
    for k in map.keys() {
        match array_index(k) {
            Some(i) => indexes.push((i, k)),
            None => rest.push(k),
        }
    }
    indexes.sort_by_key(|(i, _)| *i);
    indexes.into_iter().map(|(_, k)| k).chain(rest).collect()
}

/// A canonical array index: "0" or digits without a leading zero, below 2^32 - 1.
fn array_index(k: &str) -> Option<u32> {
    if k.is_empty() || !k.bytes().all(|b| b.is_ascii_digit()) || (k.len() > 1 && k.starts_with('0')) {
        return None;
    }
    let n: u64 = k.parse().ok()?;
    if n < 4294967295 {
        Some(n as u32)
    } else {
        None
    }
}

/// The object as enumeration would show it.
fn js_ordered(map: Map<String, Value>) -> Map<String, Value> {
    let keys: Vec<String> = js_keys(&map).into_iter().cloned().collect();
    let mut map = map;
    let mut out = Map::new();
    for k in keys {
        let v = map.shift_remove(&k).unwrap();
        out.insert(k, v);
    }
    out
}

/// `String(x)` for the values the error messages print.
fn js_string(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => js_number_string(n.as_f64().unwrap()),
        Value::String(s) => s.clone(),
        Value::Array(a) => a
            .iter()
            .map(|e| if e.is_null() { String::new() } else { js_string(e) })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_string(),
    }
}

fn js_number_string(x: f64) -> String {
    if x == 0.0 {
        return "0".to_string();
    }
    let a = x.abs();
    if (1e-6..1e21).contains(&a) {
        return format!("{x}");
    }
    let s = format!("{x:e}");
    match s.split_once('e') {
        Some((m, e)) if !e.starts_with('-') => format!("{m}e+{e}"),
        _ => s,
    }
}

/// `typeof v`
fn js_typeof(v: &Value) -> &'static str {
    match v {
        Value::Null | Value::Array(_) | Value::Object(_) => "object",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
    }
}

/// `Number.isInteger(v)` for a stream entry: the number, if it is one.
fn integer_of(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => {
            let x = n.as_f64().unwrap();
            if x.is_finite() && x.trunc() == x {
                Some(x)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// src/vendor/objpack.ts:53 `SLObjPack`
pub struct SLObjPack {
    limits: SLObjPackLimits,
    forbidden_keys: HashSet<String>,
}

impl Default for SLObjPack {
    fn default() -> Self {
        Self::new(SLObjPackOptions::default())
    }
}

impl SLObjPack {
    /// src/vendor/objpack.ts:53 `constructor`
    pub fn new(options: SLObjPackOptions) -> Self {
        let o = options.limits;
        let limits = SLObjPackLimits {
            depth: o.depth.unwrap_or(DEFAULT_LIMITS.depth),
            array_length: o.array_length.unwrap_or(DEFAULT_LIMITS.array_length),
            object_keys: o.object_keys.unwrap_or(DEFAULT_LIMITS.object_keys),
            stream_length: o.stream_length.unwrap_or(DEFAULT_LIMITS.stream_length),
            values_length: o.values_length.unwrap_or(DEFAULT_LIMITS.values_length),
            decode_ops: o.decode_ops.unwrap_or(DEFAULT_LIMITS.decode_ops),
        };
        let forbidden_keys = match options.forbidden_keys {
            Some(keys) => keys.into_iter().collect(),
            None => DEFAULT_FORBIDDEN_KEYS.iter().map(|k| k.to_string()).collect(),
        };
        SLObjPack { limits, forbidden_keys }
    }

    /// src/vendor/objpack.ts:72 `pack`. Returns `[valuesList, stream]`.
    pub fn pack(&self, json_obj: &Value) -> Result<Value, ObjPackError> {
        let limits = &self.limits;

        // -------- Pass 1: validating snapshot -------- (objpack.ts:76)
        let safe = self.snapshot(json_obj, 0)?;

        // -------- Pass 2: frequency count over the snapshot -------- (objpack.ts:159)
        let mut frequency = Frequency::default();
        count_primitives(limits, &mut frequency, &safe, 0)?;

        // objpack.ts:186 (the sort is stable, as in JavaScript)
        let mut entries: Vec<(Prim, usize)> = frequency.entries;
        entries.sort_by(|a, b| b.1.cmp(&a.1));
        let values_list: Vec<Prim> = entries.into_iter().map(|e| e.0).collect();

        let mut value_to_index: HashMap<Prim, usize> = HashMap::new();
        for (idx, val) in values_list.iter().enumerate() {
            value_to_index.insert(val.clone(), idx);
        }

        // -------- Pass 3: emit opcode stream -------- (objpack.ts:226)
        let mut stream: Vec<i64> = Vec::new();
        emit(limits, &value_to_index, &mut stream, &safe, 0)?;

        Ok(Value::Array(vec![
            Value::Array(values_list.iter().map(value_of).collect()),
            Value::Array(stream.into_iter().map(Value::from).collect()),
        ]))
    }

    /// src/vendor/objpack.ts:85 `snapshot`
    fn snapshot(&self, node: &Value, depth: usize) -> Result<Value, ObjPackError> {
        let limits = &self.limits;
        if depth as f64 > limits.depth {
            return err("SLObjPack pack: Max depth exceeded".to_string());
        }
        match node {
            Value::Null | Value::String(_) | Value::Bool(_) | Value::Number(_) => Ok(node.clone()),
            Value::Array(items) => {
                let len = items.len();
                if len as f64 > limits.array_length {
                    return err("SLObjPack pack: Array length exceeds limit".to_string());
                }
                let mut out = Vec::with_capacity(len);
                for item in items {
                    out.push(self.snapshot(item, depth + 1)?);
                }
                Ok(Value::Array(out))
            }
            Value::Object(record) => {
                let keys = js_keys(record);
                if keys.len() as f64 > limits.object_keys {
                    return err("SLObjPack pack: Object key count exceeds limit".to_string());
                }
                let mut out = Map::new();
                for k in keys {
                    if self.forbidden_keys.contains(k) {
                        return err(format!("SLObjPack pack: Forbidden key: {k}"));
                    }
                    out.insert(k.clone(), self.snapshot(&record[k.as_str()], depth + 1)?);
                }
                Ok(Value::Object(out))
            }
        }
    }

    /// src/vendor/objpack.ts:279 `unpack`
    pub fn unpack(&self, packed: &Value) -> Result<Value, ObjPackError> {
        let limits = &self.limits;

        // -------- Shell validation -------- (objpack.ts:283)
        let (values_list_raw, stream_raw) = match packed {
            Value::Array(p) if p.len() == 2 => match (&p[0], &p[1]) {
                (Value::Array(v), Value::Array(s)) => (v, s),
                _ => return err("SLObjPack unpack: Invalid payload structure".to_string()),
            },
            _ => return err("SLObjPack unpack: Invalid payload structure".to_string()),
        };

        // -------- Global size caps -------- (objpack.ts:293)
        if values_list_raw.len() as f64 > limits.values_length {
            return err("SLObjPack unpack: valuesList exceeds limit".to_string());
        }
        if stream_raw.len() as f64 > limits.stream_length {
            return err("SLObjPack unpack: stream exceeds limit".to_string());
        }

        // -------- valuesList content validation -------- (objpack.ts:303)
        for (i, v) in values_list_raw.iter().enumerate() {
            match v {
                Value::Null | Value::String(_) | Value::Bool(_) => continue,
                Value::Number(n) => {
                    if !n.as_f64().unwrap().is_finite() {
                        return err(format!("SLObjPack unpack: Non-finite number in valuesList at {i}"));
                    }
                    continue;
                }
                _ => {
                    return err(format!(
                        "SLObjPack unpack: Invalid valuesList entry at {i} (type {})",
                        js_typeof(v)
                    ))
                }
            }
        }

        let mut decoder = Decoder {
            limits,
            forbidden_keys: &self.forbidden_keys,
            values_list: values_list_raw,
            stream: stream_raw,
            cursor: 0,
        };
        let result = decoder.decode(0)?;

        // Trailing garbage rejection (objpack.ts:446)
        if decoder.cursor != stream_raw.len() {
            return err("SLObjPack unpack: Extra data after decoding".to_string());
        }

        Ok(result)
    }
}

/// `primitivesFrequency`: a `Map` keeping insertion order.
#[derive(Default)]
struct Frequency {
    entries: Vec<(Prim, usize)>,
    index: HashMap<Prim, usize>,
}

impl Frequency {
    fn bump(&mut self, prim: Prim) {
        match self.index.get(&prim) {
            Some(&i) => self.entries[i].1 += 1,
            None => {
                self.index.insert(prim.clone(), self.entries.len());
                self.entries.push((prim, 1));
            }
        }
    }
}

// src/vendor/objpack.ts:162 `countPrimitives`
fn count_primitives(limits: &SLObjPackLimits, freq: &mut Frequency, node: &Value, depth: usize) -> Result<(), ObjPackError> {
    if depth as f64 > limits.depth {
        return err("SLObjPack pack: Max depth exceeded".to_string());
    }
    match node {
        Value::Array(items) => {
            for item in items {
                count_primitives(limits, freq, item, depth + 1)?;
            }
        }
        Value::Object(obj) => {
            for k in js_keys(obj) {
                freq.bump(Prim::Str(k.clone()));
                count_primitives(limits, freq, &obj[k.as_str()], depth + 1)?;
            }
        }
        prim => freq.bump(prim_of(prim)),
    }
    Ok(())
}

// src/vendor/objpack.ts:193 `getPtr`
fn get_ptr(value_to_index: &HashMap<Prim, usize>, node: &Prim) -> Result<i64, ObjPackError> {
    match value_to_index.get(node) {
        Some(&ptr) => Ok(ptr as i64),
        None => err("SLObjPack pack: Internal error \u{2014} unindexed primitive".to_string()),
    }
}

// src/vendor/objpack.ts:206 `isSchemaArray`
fn is_schema_array(arr: &[Value]) -> Option<Vec<String>> {
    if arr.is_empty() {
        return None;
    }
    let first = match &arr[0] {
        Value::Object(o) => o,
        _ => return None,
    };
    let keys0: Vec<&String> = js_keys(first);
    if keys0.is_empty() {
        return None;
    }
    for item in &arr[1..] {
        let item = match item {
            Value::Object(o) => o,
            _ => return None,
        };
        let keys_i = js_keys(item);
        if keys_i.len() != keys0.len() {
            return None;
        }
        for k in 0..keys0.len() {
            if keys_i[k] != keys0[k] {
                return None;
            }
        }
    }
    Some(keys0.into_iter().cloned().collect())
}

// src/vendor/objpack.ts:229 `emit`
fn emit(limits: &SLObjPackLimits, value_to_index: &HashMap<Prim, usize>, stream: &mut Vec<i64>, node: &Value, depth: usize) -> Result<(), ObjPackError> {
    if depth as f64 > limits.depth {
        return err("SLObjPack pack: Max depth exceeded".to_string());
    }
    match node {
        Value::Array(arr) => {
            if arr.is_empty() {
                stream.push(-4);
                return Ok(());
            }
            if arr.len() == 1 {
                stream.push(-5);
                return emit(limits, value_to_index, stream, &arr[0], depth + 1);
            }

            if let Some(schema_keys) = is_schema_array(arr) {
                stream.push(-3);
                stream.push(arr.len() as i64);
                stream.push(schema_keys.len() as i64);
                for k in &schema_keys {
                    stream.push(get_ptr(value_to_index, &Prim::Str(k.clone()))?);
                }
                for item in arr {
                    let rec = item.as_object().unwrap();
                    for k in &schema_keys {
                        emit(limits, value_to_index, stream, &rec[k.as_str()], depth + 1)?;
                    }
                }
                return Ok(());
            }

            stream.push(-2);
            stream.push(arr.len() as i64);
            for n in arr {
                emit(limits, value_to_index, stream, n, depth + 1)?;
            }
            Ok(())
        }
        Value::Object(obj) => {
            let keys = js_keys(obj);
            if keys.is_empty() {
                stream.push(-6);
                return Ok(());
            }

            stream.push(-1);
            stream.push(keys.len() as i64);
            for k in &keys {
                stream.push(get_ptr(value_to_index, &Prim::Str((*k).clone()))?);
            }
            for k in &keys {
                emit(limits, value_to_index, stream, &obj[k.as_str()], depth + 1)?;
            }
            Ok(())
        }
        prim => {
            stream.push(get_ptr(value_to_index, &prim_of(prim))?);
            Ok(())
        }
    }
}

/// The closures of `unpack` (objpack.ts:327 on) share `cursor`, so they are methods.
struct Decoder<'a> {
    limits: &'a SLObjPackLimits,
    forbidden_keys: &'a HashSet<String>,
    values_list: &'a [Value],
    stream: &'a [Value],
    cursor: usize,
}

impl Decoder<'_> {
    // src/vendor/objpack.ts:327 `readStream`
    fn read_stream(&mut self) -> Result<Value, ObjPackError> {
        if self.cursor >= self.stream.len() {
            return err("SLObjPack unpack: Unexpected end of stream".to_string());
        }
        let v = self.stream[self.cursor].clone();
        self.cursor += 1;
        Ok(v)
    }

    // src/vendor/objpack.ts:334 `resolvePointer`
    fn resolve_pointer(&self, ptr: &Value) -> Result<Value, ObjPackError> {
        if let Some(p) = integer_of(ptr) {
            if p >= 0.0 && p < self.values_list.len() as f64 {
                return Ok(self.values_list[p as usize].clone());
            }
        }
        err(format!("SLObjPack unpack: Invalid value pointer {}", js_string(ptr)))
    }

    // src/vendor/objpack.ts:341 `readKey`
    fn read_key(&mut self) -> Result<String, ObjPackError> {
        let ptr = self.read_stream()?;
        let key = self.resolve_pointer(&ptr)?;
        let key = match key {
            Value::String(s) => s,
            other => {
                return err(format!("SLObjPack unpack: Keys must be strings, got {}", js_typeof(&other)));
            }
        };
        if self.forbidden_keys.contains(&key) {
            return err(format!("SLObjPack unpack: Forbidden key: {key}"));
        }
        Ok(key)
    }

    // src/vendor/objpack.ts:365 `validateCount`
    fn validate_count(n: &Value, max: f64, label: &str) -> Result<usize, ObjPackError> {
        if let Some(x) = integer_of(n) {
            if x >= 0.0 && x <= max {
                return Ok(x as usize);
            }
        }
        err(format!("SLObjPack unpack: Invalid {label} count: {}", js_string(n)))
    }

    // src/vendor/objpack.ts:374 `requireStream`
    fn require_stream(&self, min: f64, label: &str) -> Result<(), ObjPackError> {
        if min > (self.stream.len() - self.cursor) as f64 {
            return err(format!("SLObjPack unpack: {label} exceeds remaining stream"));
        }
        Ok(())
    }

    // src/vendor/objpack.ts:383 `decode`
    fn decode(&mut self, depth: usize) -> Result<Value, ObjPackError> {
        let limits = self.limits;
        if depth as f64 > limits.depth {
            return err("SLObjPack unpack: Max depth exceeded".to_string());
        }
        let op_value = self.read_stream()?;
        let op = match integer_of(&op_value) {
            Some(op) => op,
            None => return err(format!("SLObjPack unpack: Invalid opcode {}", js_string(&op_value))),
        };
        if op >= 0.0 {
            return self.resolve_pointer(&op_value);
        }

        if op == -1.0 {
            let n = self.read_stream()?;
            let num_keys = Self::validate_count(&n, limits.object_keys, "object key")?;
            // numKeys key pointers + numKeys values (>=1 slot each)
            self.require_stream((num_keys * 2) as f64, "object")?;
            let mut keys = Vec::with_capacity(num_keys);
            for _ in 0..num_keys {
                keys.push(self.read_key()?);
            }
            let mut obj = Map::new();
            for key in keys {
                let value = self.decode(depth + 1)?;
                obj.insert(key, value);
            }
            Ok(Value::Object(js_ordered(obj)))
        } else if op == -2.0 {
            let n = self.read_stream()?;
            let num_items = Self::validate_count(&n, limits.array_length, "array item")?;
            self.require_stream(num_items as f64, "array")?;
            let mut arr = Vec::with_capacity(num_items);
            for _ in 0..num_items {
                arr.push(self.decode(depth + 1)?);
            }
            Ok(Value::Array(arr))
        } else if op == -3.0 {
            let n = self.read_stream()?;
            let num_items = Self::validate_count(&n, limits.array_length, "schema array item")?;
            let n = self.read_stream()?;
            let num_keys = Self::validate_count(&n, limits.object_keys, "schema key")?;
            // Combined budget: individual caps multiply for `-3`.
            if (num_items as f64) * (num_keys as f64) > limits.decode_ops {
                return err("SLObjPack unpack: Schema array decode budget exceeded".to_string());
            }
            // numKeys schema key pointers + numItems*numKeys values
            self.require_stream(num_keys as f64 + (num_items as f64) * (num_keys as f64), "schema array")?;
            let mut keys = Vec::with_capacity(num_keys);
            for _ in 0..num_keys {
                keys.push(self.read_key()?);
            }
            let mut arr = Vec::with_capacity(num_items);
            for _ in 0..num_items {
                let mut obj = Map::new();
                for key in &keys {
                    let value = self.decode(depth + 1)?;
                    obj.insert(key.clone(), value);
                }
                arr.push(Value::Object(js_ordered(obj)));
            }
            Ok(Value::Array(arr))
        } else if op == -4.0 {
            Ok(Value::Array(Vec::new()))
        } else if op == -5.0 {
            Ok(Value::Array(vec![self.decode(depth + 1)?]))
        } else if op == -6.0 {
            Ok(Value::Object(Map::new()))
        } else {
            err(format!("SLObjPack unpack: Unknown opcode {}", js_number_string(op)))
        }
    }
}
