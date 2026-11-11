//! Reading and writing JSON, in a few functions.
//!
//! A payload crosses the pipe as JSON and nothing here needs a document tree: a
//! plugin reads a handful of fields out of what it was given and writes one small
//! object back. So this scans. A field is a slice of the string it came from, and
//! the only copying is a string's escapes being undone.

/// Where one value ends, so the caller can keep the text between the marks.
function skipValue(text: string, at: i32): i32 {
  let i = at;
  while (i < text.length && isSpace(text.charCodeAt(i))) i++;
  if (i >= text.length) return i;
  const c = text.charCodeAt(i);
  if (c === 0x22) return skipString(text, i);
  if (c === 0x7b || c === 0x5b) return skipContainer(text, i);
  while (i < text.length) {
    const d = text.charCodeAt(i);
    if (d === 0x2c || d === 0x5d || d === 0x7d || isSpace(d)) break;
    i++;
  }
  return i;
}

function isSpace(c: i32): bool {
  return c === 0x20 || c === 0x09 || c === 0x0a || c === 0x0d;
}

function skipString(text: string, at: i32): i32 {
  let i = at + 1;
  while (i < text.length) {
    const c = text.charCodeAt(i);
    if (c === 0x5c) {
      i += 2;
      continue;
    }
    i++;
    if (c === 0x22) return i;
  }
  return i;
}

function skipContainer(text: string, at: i32): i32 {
  const array = text.charCodeAt(at) === 0x5b;
  const close = array ? 0x5d : 0x7d;
  let i = at + 1;
  while (i < text.length) {
    while (i < text.length && isSpace(text.charCodeAt(i))) i++;
    if (i >= text.length) return i;
    if (text.charCodeAt(i) === close) return i + 1;
    if (!array) {
      i = skipString(text, i);
      while (i < text.length && isSpace(text.charCodeAt(i))) i++;
      if (i < text.length && text.charCodeAt(i) === 0x3a) i++;
    }
    i = skipValue(text, i);
    while (i < text.length && isSpace(text.charCodeAt(i))) i++;
    if (i < text.length && text.charCodeAt(i) === 0x2c) i++;
  }
  return i;
}

/// The raw text of `key`'s value, or null when the object has no such key.
export function field(json: string, key: string): string | null {
  let at = 0;
  while (at < json.length && isSpace(json.charCodeAt(at))) at++;
  if (at >= json.length || json.charCodeAt(at) !== 0x7b) return null;
  at++;
  while (at < json.length) {
    while (at < json.length && isSpace(json.charCodeAt(at))) at++;
    if (at >= json.length || json.charCodeAt(at) === 0x7d) return null;
    const quoted = at;
    at = skipString(json, at);
    const found = json.substring(quoted + 1, at - 1);
    while (at < json.length && isSpace(json.charCodeAt(at))) at++;
    if (at >= json.length || json.charCodeAt(at) !== 0x3a) return null;
    at++;
    const start = at;
    at = skipValue(json, at);
    if (found === key) return json.substring(start, at);
    while (at < json.length && isSpace(json.charCodeAt(at))) at++;
    if (at < json.length && json.charCodeAt(at) === 0x2c) at++;
  }
  return null;
}

/// A dotted path, such as `options.text`.
export function path(json: string, where: string): string | null {
  let current = json;
  const steps = where.split(".");
  for (let i = 0; i < steps.length; i++) {
    const next = field(current, steps[i]);
    if (next === null) return null;
    current = next;
  }
  return current;
}

/// The text of a JSON string, unescaped: `null` when the value isn't one.
export function text(value: string | null): string | null {
  if (value === null) return null;
  let at = 0;
  while (at < value.length && isSpace(value.charCodeAt(at))) at++;
  if (at >= value.length || value.charCodeAt(at) !== 0x22) return null;
  let out = "";
  at++;
  while (at < value.length) {
    const c = value.charCodeAt(at);
    if (c === 0x22) return out;
    if (c === 0x5c) {
      if (at + 1 >= value.length) return null;
      const escape = value.charCodeAt(at + 1);
      at += 2;
      if (escape === 0x22) out += '"';
      else if (escape === 0x5c) out += "\\";
      else if (escape === 0x2f) out += "/";
      else if (escape === 0x62) out += "\b";
      else if (escape === 0x66) out += "\f";
      else if (escape === 0x6e) out += "\n";
      else if (escape === 0x72) out += "\r";
      else if (escape === 0x74) out += "\t";
      else if (escape === 0x75) {
        // A `\u` escape is a code unit, and a surrogate pair is two of them, so
        // reading the four hex digits in order gives the character back.
        if (at + 4 > value.length) return null;
        const hex = value.substring(at, at + 4);
        out += String.fromCharCode(<u16>parseInt(hex, 16));
        at += 4;
      } else return null;
      continue;
    }
    out += value.charAt(at);
    at++;
  }
  return null;
}

/// Whether a JSON string value is exactly `want`.
export function isString(value: string | null, want: string): bool {
  if (value === null) return false;
  const read = text(value);
  return read !== null && read == want;
}

/// Writes `target` as a JSON string, quoted and escaped.
export function quote(target: string): string {
  let out = '"';
  for (let i = 0; i < target.length; i++) {
    const c = target.charCodeAt(i);
    if (c === 0x22) out += '\\"';
    else if (c === 0x5c) out += "\\\\";
    else if (c === 0x0a) out += "\\n";
    else if (c === 0x0d) out += "\\r";
    else if (c === 0x09) out += "\\t";
    else if (c < 0x20) {
      const hex = c.toString(16);
      out += "\\u00" + (c < 0x10 ? "0" + hex : hex);
    } else out += target.charAt(i);
  }
  return out + '"';
}

/// An array's items, as the raw text of each.
///
/// A batch can carry more events than a plugin wants to size an array for, and
/// this gives them all: read the ones that matter and let the rest go.
export function items(array: string | null): string[] {
  const out: string[] = [];
  if (array === null) return out;
  let at = 0;
  while (at < array.length && isSpace(array.charCodeAt(at))) at++;
  if (at >= array.length || array.charCodeAt(at) !== 0x5b) return out;
  at++;
  while (at < array.length) {
    while (at < array.length && isSpace(array.charCodeAt(at))) at++;
    if (at >= array.length || array.charCodeAt(at) === 0x5d) return out;
    const start = at;
    at = skipValue(array, at);
    out.push(array.substring(start, at));
    while (at < array.length && isSpace(array.charCodeAt(at))) at++;
    if (at < array.length && array.charCodeAt(at) === 0x2c) at++;
  }
  return out;
}
