//! Reading and writing JSON without a JSON library.
//!
//! Payloads cross the pipe as JSON and nothing on this side needs a document
//! tree: a plugin reads a handful of fields out of what it was given, and writes
//! one small object back. So this scans rather than parses. A field is a view of
//! the bytes it already has, and the only copying is a string's escapes being
//! undone into the plugin's own buffer.
//!
//! It is deliberately strict about what it accepts and lenient about what it
//! ignores: an object it can't follow is a field it doesn't have, never a crash.

#include "yapfast.h"

static bool is_space(char c) {
    return c == ' ' || c == '\t' || c == '\n' || c == '\r';
}

static bool same_bytes(const char *a, const char *b, size_t n) {
    for (size_t i = 0; i < n; i++) {
        if (a[i] != b[i]) return false;
    }
    return true;
}

static bool skip_value(const char *s, size_t n, size_t *at);

/// Moves past a string, which starts at `*at`'s quote.
static bool skip_string(const char *s, size_t n, size_t *at) {
    if (*at >= n || s[*at] != '"') return false;
    (*at)++;
    while (*at < n) {
        char c = s[*at];
        if (c == '\\') {
            (*at) += 2;
            continue;
        }
        (*at)++;
        if (c == '"') return true;
    }
    return false;
}

/// Moves past an object or an array, including everything nested in it.
static bool skip_container(const char *s, size_t n, size_t *at) {
    bool array = s[*at] == '[';
    char close = array ? ']' : '}';
    (*at)++;
    for (;;) {
        while (*at < n && is_space(s[*at])) (*at)++;
        if (*at >= n) return false;
        if (s[*at] == close) {
            (*at)++;
            return true;
        }
        if (!array) {
            if (!skip_string(s, n, at)) return false;
            while (*at < n && is_space(s[*at])) (*at)++;
            if (*at >= n || s[*at] != ':') return false;
            (*at)++;
        }
        if (!skip_value(s, n, at)) return false;
        while (*at < n && is_space(s[*at])) (*at)++;
        if (*at < n && s[*at] == ',') (*at)++;
    }
}

/// Moves past one value, whatever kind it is.
static bool skip_value(const char *s, size_t n, size_t *at) {
    while (*at < n && is_space(s[*at])) (*at)++;
    if (*at >= n) return false;
    char c = s[*at];
    if (c == '"') return skip_string(s, n, at);
    if (c == '{' || c == '[') return skip_container(s, n, at);
    size_t start = *at;
    while (*at < n) {
        char d = s[*at];
        if (d == ',' || d == ']' || d == '}' || is_space(d)) break;
        (*at)++;
    }
    return *at > start;
}

bool yf_get(yf_str json, yf_str key, yf_str *value) {
    const char *s = json.data;
    size_t n = json.len;
    size_t at = 0;
    while (at < n && is_space(s[at])) at++;
    if (at >= n || s[at] != '{') return false;
    at++;
    for (;;) {
        while (at < n && is_space(s[at])) at++;
        if (at >= n || s[at] == '}') return false;
        size_t quoted = at;
        if (!skip_string(s, n, &at)) return false;
        // The key's own bytes, between its quotes. A key this ABI writes is
        // never escaped, so comparing the raw bytes is enough.
        yf_str found = { s + quoted + 1, at - quoted - 2 };
        while (at < n && is_space(s[at])) at++;
        if (at >= n || s[at] != ':') return false;
        at++;
        size_t start = at;
        if (!skip_value(s, n, &at)) return false;
        if (found.len == key.len && same_bytes(found.data, key.data, key.len)) {
            value->data = s + start;
            value->len = at - start;
            return true;
        }
        while (at < n && is_space(s[at])) at++;
        if (at < n && s[at] == ',') at++;
    }
}

bool yf_path(yf_str json, yf_str path, yf_str *value) {
    yf_str current = json;
    size_t at = 0;
    for (;;) {
        size_t start = at;
        while (at < path.len && path.data[at] != '.') at++;
        yf_str step = { path.data + start, at - start };
        if (!yf_get(current, step, &current)) return false;
        if (at >= path.len) {
            *value = current;
            return true;
        }
        at++;
    }
}

bool yf_string_is(yf_str value, yf_str want) {
    const char *s = value.data;
    size_t n = value.len;
    size_t at = 0;
    while (at < n && is_space(s[at])) at++;
    if (at >= n || s[at] != '"') return false;
    at++;
    // Only an unescaped string compares equal; anything else isn't the literal.
    return n - at == want.len + 1 && same_bytes(s + at, want.data, want.len) &&
           s[at + want.len] == '"';
}

bool yf_uint(yf_str value, uint64_t *out) {
    const char *s = value.data;
    size_t n = value.len;
    size_t at = 0;
    while (at < n && is_space(s[at])) at++;
    uint64_t number = 0;
    size_t digits = 0;
    while (at < n && s[at] >= '0' && s[at] <= '9') {
        // A count this large is not one this ABI has; wrapping would invent one.
        if (number > (UINT64_MAX - (uint64_t)(s[at] - '0')) / 10) return false;
        number = number * 10 + (uint64_t)(s[at] - '0');
        at++;
        digits++;
    }
    while (at < n && is_space(s[at])) at++;
    if (digits == 0 || at != n) return false;
    *out = number;
    return true;
}

bool yf_bool(yf_str value, bool *out) {
    const char *s = value.data;
    size_t n = value.len;
    size_t at = 0;
    while (at < n && is_space(s[at])) at++;
    while (at < n && is_space(s[n - 1])) n--;
    if (n - at == 4 && same_bytes(s + at, "true", 4)) {
        *out = true;
        return true;
    }
    if (n - at == 5 && same_bytes(s + at, "false", 5)) {
        *out = false;
        return true;
    }
    return false;
}

static bool read_hex4(const char *s, size_t n, size_t *at, uint32_t *out) {
    if (*at + 4 > n) return false;
    uint32_t value = 0;
    for (size_t i = 0; i < 4; i++) {
        char c = s[*at + i];
        uint32_t digit;
        if (c >= '0' && c <= '9') {
            digit = (uint32_t)(c - '0');
        } else if (c >= 'a' && c <= 'f') {
            digit = (uint32_t)(c - 'a') + 10;
        } else if (c >= 'A' && c <= 'F') {
            digit = (uint32_t)(c - 'A') + 10;
        } else {
            return false;
        }
        value = (value << 4) | digit;
    }
    *at += 4;
    *out = value;
    return true;
}

/// Writes one code point as UTF-8, unless it doesn't fit.
static bool write_utf8(char *out, size_t cap, size_t *at, uint32_t code) {
    if (code < 0x80) {
        if (*at + 1 > cap) return false;
        out[(*at)++] = (char)code;
    } else if (code < 0x800) {
        if (*at + 2 > cap) return false;
        out[(*at)++] = (char)(0xC0 | (code >> 6));
        out[(*at)++] = (char)(0x80 | (code & 0x3F));
    } else if (code < 0x10000) {
        if (*at + 3 > cap) return false;
        out[(*at)++] = (char)(0xE0 | (code >> 12));
        out[(*at)++] = (char)(0x80 | ((code >> 6) & 0x3F));
        out[(*at)++] = (char)(0x80 | (code & 0x3F));
    } else {
        if (*at + 4 > cap) return false;
        out[(*at)++] = (char)(0xF0 | (code >> 18));
        out[(*at)++] = (char)(0x80 | ((code >> 12) & 0x3F));
        out[(*at)++] = (char)(0x80 | ((code >> 6) & 0x3F));
        out[(*at)++] = (char)(0x80 | (code & 0x3F));
    }
    return true;
}

ptrdiff_t yf_string(yf_str value, char *out, size_t cap) {
    const char *s = value.data;
    size_t n = value.len;
    size_t at = 0;
    size_t wrote = 0;
    while (at < n && is_space(s[at])) at++;
    if (at >= n || s[at] != '"') return -1;
    at++;
    while (at < n && s[at] != '"') {
        char c = s[at];
        if (c != '\\') {
            at++;
            if (wrote + 1 > cap) return -1;
            out[wrote++] = c;
            continue;
        }
        if (at + 1 >= n) return -1;
        char escape = s[at + 1];
        at += 2;
        switch (escape) {
        case '"': c = '"'; break;
        case '\\': c = '\\'; break;
        case '/': c = '/'; break;
        case 'b': c = '\b'; break;
        case 'f': c = '\f'; break;
        case 'n': c = '\n'; break;
        case 'r': c = '\r'; break;
        case 't': c = '\t'; break;
        case 'u': {
            uint32_t code = 0;
            if (!read_hex4(s, n, &at, &code)) return -1;
            if (code >= 0xD800 && code <= 0xDBFF) {
                // A high half is only half a character: the low half follows,
                // or the pair is broken and reads as the replacement character.
                size_t pair = at;
                uint32_t low = 0;
                if (at + 1 < n && s[at] == '\\' && s[at + 1] == 'u') {
                    at += 2;
                    if (!read_hex4(s, n, &at, &low)) return -1;
                }
                if (low >= 0xDC00 && low <= 0xDFFF) {
                    code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                } else {
                    at = pair;
                    code = 0xFFFD;
                }
            } else if (code >= 0xDC00 && code <= 0xDFFF) {
                code = 0xFFFD;
            }
            if (!write_utf8(out, cap, &wrote, code)) return -1;
            continue;
        }
        default:
            return -1;
        }
        if (wrote + 1 > cap) return -1;
        out[wrote++] = c;
    }
    return (ptrdiff_t)wrote;
}

void yf_iter_init(yf_iter *it, yf_str array) {
    *it = (yf_iter){ array, 0, false };
}

bool yf_iter_next(yf_iter *it, yf_str *item) {
    const char *s = it->json.data;
    size_t n = it->json.len;
    if (!it->started) {
        while (it->at < n && is_space(s[it->at])) it->at++;
        if (it->at >= n || s[it->at] != '[') return false;
        it->at++;
        it->started = true;
    }
    while (it->at < n && is_space(s[it->at])) it->at++;
    if (it->at >= n || s[it->at] == ']') return false;
    size_t start = it->at;
    if (!skip_value(s, n, &it->at)) return false;
    item->data = s + start;
    item->len = it->at - start;
    while (it->at < n && is_space(s[it->at])) it->at++;
    if (it->at < n && s[it->at] == ',') it->at++;
    return true;
}

size_t yf_items(yf_str json, yf_str *items, size_t max) {
    yf_iter it;
    yf_iter_init(&it, json);
    size_t found = 0;
    yf_str item;
    while (yf_iter_next(&it, &item)) {
        if (found < max) items[found] = item;
        found++;
    }
    return found;
}

void yf_buf_init(yf_buf *buf, char *data, size_t cap) {
    *buf = (yf_buf){ data, cap, 0, false };
}

void yf_raw(yf_buf *buf, yf_str json) {
    if (buf->failed) return;
    if (buf->len + json.len > buf->cap) {
        buf->failed = true;
        return;
    }
    for (size_t i = 0; i < json.len; i++) buf->data[buf->len++] = json.data[i];
}

static void writes_hex(yf_buf *buf, unsigned value) {
    static const char digits[] = "0123456789abcdef";
    char out[6] = { '\\', 'u', '0', '0', digits[(value >> 4) & 0xF], digits[value & 0xF] };
    yf_raw(buf, (yf_str){ out, sizeof(out) });
}

void yf_quoted(yf_buf *buf, yf_str text) {
    yf_raw(buf, YF_STR("\""));
    for (size_t i = 0; i < text.len; i++) {
        unsigned char c = (unsigned char)text.data[i];
        switch (c) {
        case '"': yf_raw(buf, YF_STR("\\\"")); break;
        case '\\': yf_raw(buf, YF_STR("\\\\")); break;
        case '\b': yf_raw(buf, YF_STR("\\b")); break;
        case '\f': yf_raw(buf, YF_STR("\\f")); break;
        case '\n': yf_raw(buf, YF_STR("\\n")); break;
        case '\r': yf_raw(buf, YF_STR("\\r")); break;
        case '\t': yf_raw(buf, YF_STR("\\t")); break;
        default:
            // JSON has no control characters in a string, and a page of them
            // would be unreadable in a log: write the short form.
            if (c < 0x20) {
                writes_hex(buf, c);
            } else {
                yf_raw(buf, (yf_str){ text.data + i, 1 });
            }
        }
    }
    yf_raw(buf, YF_STR("\""));
}

void yf_number(yf_buf *buf, uint64_t number) {
    char digits[20];
    size_t at = sizeof(digits);
    do {
        digits[--at] = (char)('0' + (number % 10));
        number /= 10;
    } while (number != 0);
    yf_raw(buf, (yf_str){ digits + at, sizeof(digits) - at });
}

void yf_boolean(yf_buf *buf, bool value) {
    yf_raw(buf, value ? YF_STR("true") : YF_STR("false"));
}

void yf_none(yf_buf *buf) {
    yf_raw(buf, YF_STR("\"none\""));
}

void yf_notice(yf_buf *buf, yf_str text) {
    yf_raw(buf, YF_STR("{\"notice\":{\"text\":"));
    yf_quoted(buf, text);
    yf_raw(buf, YF_STR("}}"));
}

void yf_failed(yf_buf *buf, yf_str message) {
    yf_raw(buf, YF_STR("{\"failed\":{\"message\":"));
    yf_quoted(buf, message);
    yf_raw(buf, YF_STR("}}"));
}

void yf_value(yf_buf *buf, yf_str json) {
    yf_raw(buf, YF_STR("{\"value\":"));
    yf_raw(buf, json);
    yf_raw(buf, YF_STR("}"));
}

void yf_unchanged(yf_buf *buf) {
    yf_raw(buf, YF_STR("\"unchanged\""));
}

void yf_block(yf_buf *buf, yf_str reason) {
    yf_raw(buf, YF_STR("{\"block\":{\"reason\":"));
    yf_quoted(buf, reason);
    yf_raw(buf, YF_STR("}}"));
}

void yf_changed_draft(yf_buf *buf,
                      yf_str room,
                      yf_str body,
                      yf_str reply_to,
                      yf_str attachments) {
    yf_raw(buf, YF_STR("{\"changed\":{\"draft\":{\"room\":"));
    yf_quoted(buf, room);
    yf_raw(buf, YF_STR(",\"body\":"));
    yf_quoted(buf, body);
    if (reply_to.data != NULL) {
        yf_raw(buf, YF_STR(",\"reply_to\":"));
        yf_quoted(buf, reply_to);
    }
    yf_raw(buf, YF_STR(",\"attachments\":"));
    yf_raw(buf, attachments.data != NULL ? attachments : YF_STR("[]"));
    yf_raw(buf, YF_STR("}}}"));
}
