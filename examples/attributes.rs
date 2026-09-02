//! Attributes on an event and on its fields.
//!
//! An attribute is a { key, value } pair a tracer may act on. Two
//! things happen to one which reaches LTTng-UST.
//!
//! It is carried into the CTF2 metadata as a user attribute, with the
//! key split at its last dot into a namespace and a name, so
//! `std.integer.base` arrives as `"std.integer": { "base": 16 }`. That
//! happens to every attribute, whether or not anything understands it.
//!
//! And a few are understood. `std.integer.base` is the one reachable
//! from here: 2, 8, 10 or 16, which becomes the CTF2
//! `preferred-display-base` and is what a reader prints the field in.
//! `std.blob.media-type` and `lttng.fmt.print-value` are the others,
//! and want a byte array and an enumeration, neither of which this
//! crate can describe yet.
//!
//! They are written where C writes them: trailing, after the thing they
//! belong to. A field's follow its type in brackets, and an event's
//! follow its field list.

use libside::*;

define_event!(
    standalone,
    provider: "rust",
    event: "standalone",
    level: SIDE_LOGLEVEL_INFO,
    fields: (
        plain: u32,
        in_hex: u32 [side_attr("std.integer.base", side_attr_u8(16))],
        message: &str [
            side_attr("std.string.note", side_attr_string("carried into the metadata")),
            side_attr("std.string.flag", side_attr_bool(true)),
        ],
    ),
    attributes: [
        side_attr("std.event.note", side_attr_string("on the event itself")),
        side_attr("std.event.count", side_attr_s32(-7)),
    ],
);

#[libside::events]
mod trace {
    use super::*;

    define_event!(
        grouped,
        provider: "rust",
        event: "grouped",
        level: SIDE_LOGLEVEL_INFO,
        fields: (
            in_octal: u32 [side_attr("std.integer.base", side_attr_u8(8))],
            in_binary: u32 [side_attr("std.integer.base", side_attr_u8(2))],
        ),
        attributes: [side_attr("std.event.note", side_attr_string("a group's event"))],
    );
}

fn main() {
    standalone!(42, 0xbeef, "hello");
    side_event!(trace::grouped, 0o100, 0b1011);
}
