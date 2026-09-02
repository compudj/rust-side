extern crate alloc;

use alloc::boxed::Box;
use alloc::ffi::CString;
use core::any::Any;
use core::ffi::{c_char, c_void, CStr};
use core::mem::{offset_of, size_of};
use core::sync::atomic::AtomicUsize;
use core::ptr::null;

#[cfg(not(any(target_pointer_width = "32", target_pointer_width = "64")))]
compile_error!("libside-rust currently supports 32-bit and 64-bit targets only");

/// Words of a `side_ptr_t`, which is a fixed 16 bytes whatever a pointer is.
const SIDE_PTR_WORDS: usize = 16 / size_of::<*const c_void>();

/// Words of a `side_ptr_rel_t`, which is a fixed 8 bytes for the same reason.
const SIDE_REL_WORDS: usize = 8 / size_of::<*const c_void>();

pub const SIDE_EVENT_STATE_ABI_VERSION: u32 = 0;
pub const SIDE_EVENT_DESCRIPTION_ABI_VERSION: u32 = 0;

pub const SIDE_TYPE_NULL: u16 = 0;
pub const SIDE_TYPE_BOOL: u16 = 1;
pub const SIDE_TYPE_U8: u16 = 2;
pub const SIDE_TYPE_U16: u16 = 3;
pub const SIDE_TYPE_U32: u16 = 4;
pub const SIDE_TYPE_U64: u16 = 5;
pub const SIDE_TYPE_S8: u16 = 7;
pub const SIDE_TYPE_S16: u16 = 8;
pub const SIDE_TYPE_S32: u16 = 9;
pub const SIDE_TYPE_S64: u16 = 10;
pub const SIDE_TYPE_BYTE: u16 = 12;
pub const SIDE_TYPE_POINTER: u16 = 13;
pub const SIDE_TYPE_STRING_UTF8: u16 = 18;
pub const SIDE_TYPE_GATHER_BOOL: u16 = 29;
pub const SIDE_TYPE_GATHER_INTEGER: u16 = 30;
pub const SIDE_TYPE_GATHER_POINTER: u16 = 32;
pub const SIDE_TYPE_GATHER_STRUCT: u16 = 35;
pub const SIDE_TYPE_GATHER_ARRAY: u16 = 36;
pub const SIDE_TYPE_GATHER_VLA: u16 = 37;

pub const SIDE_TYPE_GATHER_ACCESS_DIRECT: u8 = 0;
pub const SIDE_TYPE_GATHER_ACCESS_POINTER: u8 = 1;

pub const SIDE_TYPE_BYTE_ORDER_LE: u8 = 0;
pub const SIDE_TYPE_BYTE_ORDER_BE: u8 = 1;

#[cfg(target_endian = "little")]
pub const SIDE_TYPE_BYTE_ORDER_HOST: u8 = SIDE_TYPE_BYTE_ORDER_LE;

#[cfg(target_endian = "big")]
pub const SIDE_TYPE_BYTE_ORDER_HOST: u8 = SIDE_TYPE_BYTE_ORDER_BE;

pub const SIDE_ATTR_TYPE_NULL: u32 = 0;
pub const SIDE_ATTR_TYPE_BOOL: u32 = 1;
pub const SIDE_ATTR_TYPE_U8: u32 = 2;
pub const SIDE_ATTR_TYPE_U16: u32 = 3;
pub const SIDE_ATTR_TYPE_U32: u32 = 4;
pub const SIDE_ATTR_TYPE_U64: u32 = 5;
pub const SIDE_ATTR_TYPE_S8: u32 = 7;
pub const SIDE_ATTR_TYPE_S16: u32 = 8;
pub const SIDE_ATTR_TYPE_S32: u32 = 9;
pub const SIDE_ATTR_TYPE_S64: u32 = 10;
pub const SIDE_ATTR_TYPE_STRING: u32 = 16;

pub const SIDE_NR_TYPE_LABEL: u16 = 48;
pub const SIDE_NR_ATTR_TYPE: u16 = 17;

pub const SIDE_LOGLEVEL_EMERG: u32 = 0;
pub const SIDE_LOGLEVEL_ALERT: u32 = 1;
pub const SIDE_LOGLEVEL_CRIT: u32 = 2;
pub const SIDE_LOGLEVEL_ERR: u32 = 3;
pub const SIDE_LOGLEVEL_WARNING: u32 = 4;
pub const SIDE_LOGLEVEL_NOTICE: u32 = 5;
pub const SIDE_LOGLEVEL_INFO: u32 = 6;
pub const SIDE_LOGLEVEL_DEBUG: u32 = 7;

/*
 * The description ABI, as libside declares it.
 *
 * These types are here to be measured, not to be written through: the
 * description is built as bytes by the const evaluator below, and every
 * offset it writes at is taken from these declarations with
 * offset_of!(), so a change to the libside headers which moves a member
 * moves what the builder writes with it.
 */

/// `side_ptr_t`: an address, in a fixed 16 bytes.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SideRawPtr {
    pub v: [*const c_void; SIDE_PTR_WORDS],
}

unsafe impl Sync for SideRawPtr {}

impl SideRawPtr {
    pub const fn null() -> Self {
        Self {
            v: [null(); SIDE_PTR_WORDS],
        }
    }

    pub const fn from_ptr(ptr: *const c_void) -> Self {
        let mut v = [null(); SIDE_PTR_WORDS];
        #[cfg(target_endian = "little")]
        {
            v[0] = ptr;
        }
        #[cfg(target_endian = "big")]
        {
            v[SIDE_PTR_WORDS - 1] = ptr;
        }
        Self { v }
    }

    pub const fn from_const<T>(ptr: *const T) -> Self {
        Self::from_ptr(ptr.cast())
    }
}

/// `side_ptr_rel_t`: the distance from this member to what it points at,
/// so that a description costs no relocation and the pages it lives on
/// stay clean and shared between processes.
#[repr(C)]
#[derive(Clone, Copy)]
pub union SideRelPtr {
    pub off: i64,
    pub rel_v: [*const c_void; SIDE_REL_WORDS],
}

/// `side_array_rel_t`: elements reached by a distance, and a length.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideRelArray {
    pub elements: SideRelPtr,
    pub length: u32,
}

/// The value part of a `side_ptr_sel_t`: either an address or a distance.
#[repr(C)]
#[derive(Clone, Copy)]
pub union SideSelPtrValue {
    pub v: [*const c_void; SIDE_PTR_WORDS],
    pub off: i64,
    pub rel_v: [*const c_void; SIDE_REL_WORDS],
}

/// `side_ptr_sel_t`: what a description and a dynamic argument both
/// write, with a selector byte saying which of the two it holds.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideSelPtr {
    pub u: SideSelPtrValue,
    pub is_offset: u8,
}

/// `side_array_sel_t`.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideSelArray {
    pub elements: SideSelPtr,
    pub length: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union SideBoolValue {
    pub side_bool8: u8,
    pub side_bool16: u16,
    pub side_bool32: u32,
    pub side_bool64: u64,
    pub padding: [u8; 32],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union SideIntegerValue {
    pub side_u8: u8,
    pub side_u16: u16,
    pub side_u32: u32,
    pub side_u64: u64,
    pub side_s8: i8,
    pub side_s16: i16,
    pub side_s32: i32,
    pub side_s64: i64,
    pub side_uptr: usize,
    pub padding: [u8; 32],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union SideArgStatic {
    pub bool_value: SideBoolValue,
    pub byte_value: u8,
    pub string_value: SideRawPtr,
    pub integer_value: SideIntegerValue,
    pub padding: [u8; 32],
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub union SideArgPayload {
    pub side_static: SideArgStatic,
    pub padding: [u8; 60],
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideArg {
    pub type_: u16,
    pub flags: u16,
    pub u: SideArgPayload,
}

impl SideArg {
    pub fn null() -> Self {
        Self {
            type_: SIDE_TYPE_NULL,
            flags: 0,
            u: SideArgPayload {
                side_static: SideArgStatic { padding: [0; 32] },
            },
        }
    }

    pub fn bool(value: bool) -> Self {
        Self {
            type_: SIDE_TYPE_BOOL,
            flags: 0,
            u: SideArgPayload {
                side_static: SideArgStatic {
                    bool_value: SideBoolValue {
                        side_bool8: value as u8,
                    },
                },
            },
        }
    }

    pub fn u8(value: u8) -> Self {
        Self {
            type_: SIDE_TYPE_U8,
            flags: 0,
            u: SideArgPayload {
                side_static: SideArgStatic {
                    integer_value: SideIntegerValue { side_u8: value },
                },
            },
        }
    }

    pub fn u16(value: u16) -> Self {
        Self {
            type_: SIDE_TYPE_U16,
            flags: 0,
            u: SideArgPayload {
                side_static: SideArgStatic {
                    integer_value: SideIntegerValue { side_u16: value },
                },
            },
        }
    }

    pub fn u32(value: u32) -> Self {
        Self {
            type_: SIDE_TYPE_U32,
            flags: 0,
            u: SideArgPayload {
                side_static: SideArgStatic {
                    integer_value: SideIntegerValue { side_u32: value },
                },
            },
        }
    }

    pub fn u64(value: u64) -> Self {
        Self {
            type_: SIDE_TYPE_U64,
            flags: 0,
            u: SideArgPayload {
                side_static: SideArgStatic {
                    integer_value: SideIntegerValue { side_u64: value },
                },
            },
        }
    }

    pub fn s8(value: i8) -> Self {
        Self {
            type_: SIDE_TYPE_S8,
            flags: 0,
            u: SideArgPayload {
                side_static: SideArgStatic {
                    integer_value: SideIntegerValue { side_s8: value },
                },
            },
        }
    }

    pub fn s16(value: i16) -> Self {
        Self {
            type_: SIDE_TYPE_S16,
            flags: 0,
            u: SideArgPayload {
                side_static: SideArgStatic {
                    integer_value: SideIntegerValue { side_s16: value },
                },
            },
        }
    }

    pub fn s32(value: i32) -> Self {
        Self {
            type_: SIDE_TYPE_S32,
            flags: 0,
            u: SideArgPayload {
                side_static: SideArgStatic {
                    integer_value: SideIntegerValue { side_s32: value },
                },
            },
        }
    }

    pub fn s64(value: i64) -> Self {
        Self {
            type_: SIDE_TYPE_S64,
            flags: 0,
            u: SideArgPayload {
                side_static: SideArgStatic {
                    integer_value: SideIntegerValue { side_s64: value },
                },
            },
        }
    }

    pub fn pointer<T>(value: *const T) -> Self {
        Self {
            type_: SIDE_TYPE_POINTER,
            flags: 0,
            u: SideArgPayload {
                side_static: SideArgStatic {
                    integer_value: SideIntegerValue {
                        side_uptr: value as usize,
                    },
                },
            },
        }
    }

    pub fn string(value: &CStr) -> Self {
        Self {
            type_: SIDE_TYPE_STRING_UTF8,
            flags: 0,
            u: SideArgPayload {
                side_static: SideArgStatic {
                    string_value: SideRawPtr::from_const(value.as_ptr()),
                },
            },
        }
    }

    pub fn gather_struct<T>(value: *const T) -> Self {
        Self {
            type_: SIDE_TYPE_GATHER_STRUCT,
            flags: 0,
            u: SideArgPayload {
                side_static: SideArgStatic {
                    string_value: SideRawPtr::from_const(value),
                },
            },
        }
    }
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideArgVec {
    pub sav: SideRawPtr,
    pub len: u32,
}

impl SideArgVec {
    pub fn new(args: &[SideArg]) -> Self {
        Self {
            sav: if args.is_empty() {
                SideRawPtr::null()
            } else {
                SideRawPtr::from_const(args.as_ptr())
            },
            len: args.len() as u32,
        }
    }
}

/// `struct side_type_raw_string`: a string a description reaches by a
/// distance and a dynamic argument by an address.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideTypeRawString {
    pub p: SideSelPtr,
    pub unit_size: u8,
    pub byte_order: u8,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub union SideAttrValuePayload {
    pub bool_value: u8,
    pub string_value: SideTypeRawString,
    pub integer_value: SideIntegerValue,
    pub padding: [u8; 32],
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideAttrValue {
    pub type_: u32,
    pub u: SideAttrValuePayload,
}

/// `struct side_attr`: one { key, value } pair of an attribute list.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideAttr {
    pub key: SideTypeRawString,
    pub value: SideAttrValue,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideTypeNull {
    pub attributes: SideSelArray,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideTypeBool {
    pub attributes: SideSelArray,
    pub bool_size: u16,
    pub len_bits: u16,
    pub byte_order: u8,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideTypeByte {
    pub attributes: SideSelArray,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideTypeString {
    pub attributes: SideSelArray,
    pub unit_size: u8,
    pub byte_order: u8,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideTypeInteger {
    pub attributes: SideSelArray,
    pub integer_size: u16,
    pub len_bits: u16,
    pub signedness: u8,
    pub byte_order: u8,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideTypeStruct {
    pub fields: SideRelArray,
    pub attributes: SideRelArray,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideTypeArray {
    pub elem_type: SideRelPtr,
    pub length: u32,
    pub attributes: SideRelArray,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideTypeVla {
    pub elem_type: SideRelPtr,
    pub length_type: SideRelPtr,
    pub attributes: SideRelArray,
}

/// A variable-length array view used inside a gather struct.
///
/// Construct this from a slice whose backing storage remains valid for the
/// duration of the event call.
#[doc(hidden)]
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SideVla<T> {
    pub ptr: *const T,
    pub len: usize,
}

impl<T> SideVla<T> {
    pub fn from_slice(slice: &[T]) -> Self {
        Self {
            ptr: slice.as_ptr(),
            len: slice.len(),
        }
    }
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideTypeGatherBool {
    pub offset: u64,
    pub offset_bits: u16,
    pub access_mode: u8,
    pub type_: SideTypeBool,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideTypeGatherInteger {
    pub offset: u64,
    pub offset_bits: u16,
    pub access_mode: u8,
    pub type_: SideTypeInteger,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideTypeGatherStruct {
    /*
     * A structure has a name of its own, so it may be defined
     * elsewhere: this says whether it holds the distance to it or its
     * address. Everything this crate builds is in the description it
     * belongs to, so it is always a distance here.
     */
    pub type_: SideSelPtr,
    pub offset: u64,
    pub access_mode: u8,
    pub size: u32,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideTypeGatherArray {
    pub offset: u64,
    pub access_mode: u8,
    pub type_: SideTypeArray,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideTypeGatherVla {
    pub offset: u64,
    pub access_mode: u8,
    pub type_: SideTypeVla,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub union SideTypeGatherPayload {
    pub side_bool: SideTypeGatherBool,
    pub side_integer: SideTypeGatherInteger,
    pub side_struct: SideTypeGatherStruct,
    pub side_array: SideTypeGatherArray,
    pub side_vla: SideTypeGatherVla,
    pub padding: [u8; 61],
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideTypeGather {
    pub u: SideTypeGatherPayload,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union SideTypePayload {
    pub side_null: SideTypeNull,
    pub side_bool: SideTypeBool,
    pub side_byte: SideTypeByte,
    pub side_string: SideTypeString,
    pub side_integer: SideTypeInteger,
    pub side_gather: SideTypeGather,
    pub padding: [u8; 62],
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideType {
    pub type_: u16,
    pub u: SideTypePayload,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideEventField {
    pub field_name: SideRelPtr,
    pub side_type: SideType,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SideEventState {
    pub version: u32,
}

/*
 * The state is what a tracer writes to, and the edge between an event
 * and its description runs from here rather than the other way around:
 * a description holds no address at all, not even of its own state.
 */
#[repr(C)]
pub struct SideEventState0 {
    pub parent: SideEventState,
    pub nr_callbacks: u32,
    /*
     * Written by a tracer, in another thread, when it enables or
     * disables the event, and read at every call site. An atomic of the
     * width libside declares it, `uintptr_t enabled', so that reading
     * it while a tracer writes it is a defined thing to do rather than
     * a race the compiler is entitled to assume cannot happen.
     */
    pub enabled: AtomicUsize,
    pub callbacks: *const c_void,
    pub desc: *mut SideEventDescription,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideEventDescription {
    pub struct_size: u32,
    pub version: u32,
    pub provider_name: SideRelPtr,
    pub event_name: SideRelPtr,
    pub fields: SideRelArray,
    pub attributes: SideRelArray,
    pub flags: u64,
    pub nr_side_type_label: u16,
    pub nr_side_attr_type: u16,
    pub loglevel: u32,
}

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct SideEventStatePtr(pub *mut SideEventState);

unsafe impl Sync for SideEventStatePtr {}

#[repr(C)]
pub struct SideEventsRegisterHandle {
    _private: [u8; 0],
}

extern "C" {
    pub fn side_call(state: *const SideEventState, side_arg_vec: *const SideArgVec);
    pub fn side_events_register(
        events: *mut *mut SideEventState,
        nr_events: u32,
    ) -> *mut SideEventsRegisterHandle;
    pub fn side_events_unregister(handle: *mut SideEventsRegisterHandle);
    pub static side_empty_callback: c_char;
}

pub unsafe fn call(state: *const SideEventState, args: &[SideArg]) {
    let side_arg_vec = SideArgVec::new(args);
    unsafe {
        side_call(state, &side_arg_vec);
    }
}

/*
 * Describing a field.
 *
 * A libside type description is no longer something which can be
 * written once and shared: every pointer within it is the distance from
 * the member holding it to what it points at, so the bytes of a type
 * depend on where that type is placed. A type therefore cannot be a
 * constant of the Rust type it describes, as it was when the members
 * were addresses; what is constant is the *shape*, and the description
 * is laid out from it, at build time, for each place it appears.
 */

/// What the description of one field looks like.
#[derive(Clone, Copy)]
pub enum Layout {
    /* Stack-copy types: the value is pushed at the call site. */
    Bool,
    Integer { size: u16, signed: bool },
    Pointer,
    String,

    /*
     * Gather types: the call site pushes a base address and the
     * description says what to read from it.
     */
    GatherBool {
        offset: u64,
        size: u16,
    },
    GatherInteger {
        offset: u64,
        size: u16,
        signed: bool,
    },
    GatherPointer {
        offset: u64,
    },
    GatherStruct {
        offset: u64,
        size: u32,
        access: u8,
        fields: &'static [Field],
    },
    GatherArray {
        offset: u64,
        length: u32,
        elem: &'static Layout,
    },
    GatherVla {
        offset: u64,
        len_offset: u64,
        elem: &'static Layout,
    },

    /*
     * A structure described in another object: another group of events,
     * or another crate. Its description cannot be reached by a distance
     * -- a distance is between two bytes of one object -- so the
     * reference holds its address, which is left as a hole for the
     * loader to fill; see `Patch'. `target' names which structure, by
     * position in the list of those a group refers to, and is here so
     * that two references which describe the same way but name
     * different structures are not taken for one.
     */
    ExternStruct {
        offset: u64,
        size: u32,
        access: u8,
        target: usize,
    },
}

/// Where a foreign address goes.
///
/// The const evaluator refuses to turn a pointer into an integer, so an
/// address cannot be written into the bytes of a description at build
/// time, whatever it points at. The bytes are therefore laid out with a
/// hole where each one belongs, and what the description is made of is
/// the runs between the holes and a pointer at each: a pointer written
/// as a pointer, which is a relocation, which the loader fills in and
/// which a reader of the file can follow.
///
/// The holes come out in the order they are laid out, each one
/// `SideRawPtr` wide, which is what `description_run()` and the object
/// a call site builds around them rely on.
#[derive(Clone, Copy)]
pub struct Patch {
    /// The byte of the description where the address belongs.
    pub at: usize,
}

/// How wide a hole is: what an address occupies in a selector pointer.
pub const PATCH_WIDTH: usize = size_of::<SideRawPtr>();

/// The bytes of a description from `from`, which is the run between two
/// holes, or before the first, or after the last.
pub const fn description_run<const N: usize>(bytes: &[u8], from: usize) -> [u8; N] {
    let mut run = [0u8; N];
    let mut i = 0;
    while i < N {
        run[i] = bytes[from + i];
        i += 1;
    }
    run
}

/// What the const evaluator builds: the bytes, and the `K` addresses
/// left for the constructor to write into them.
pub struct Built<const N: usize, const K: usize> {
    pub bytes: [u8; N],
    pub patches: [Patch; K],
}

struct Patches<const K: usize> {
    at: [Patch; K],
    len: usize,
}

impl<const K: usize> Patches<K> {
    const fn new() -> Self {
        Self {
            at: [Patch { at: 0 }; K],
            len: 0,
        }
    }
}

const fn patch<const K: usize>(patches: &mut Patches<K>, at: usize) {
    assert!(
        patches.len < K,
        "side: more references to a structure described elsewhere than were counted"
    );
    /*
     * In order, and never overlapping: the runs between them are what
     * the description is cut into, and a call site names one pointer
     * per hole in the order the fields which need them were written.
     */
    if patches.len > 0 {
        assert!(
            patches.at[patches.len - 1].at + PATCH_WIDTH <= at,
            "side: the addresses of a description are not laid out in order"
        );
    }
    patches.at[patches.len] = Patch { at };
    patches.len += 1;
}

/// What an attribute holds.
///
/// The types libside gives an attribute value, of which these are the
/// ones a description built here can carry; a float or a 128 bit
/// integer would go the same way.
#[derive(Clone, Copy)]
pub enum AttrValue {
    Null,
    Bool(bool),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    S8(i8),
    S16(i16),
    S32(i32),
    S64(i64),
    String(&'static str),
}

/// One attribute of an event or of a type: a key and a value.
#[derive(Clone, Copy)]
pub struct Attr {
    pub key: &'static str,
    pub value: AttrValue,
}

/// An attribute, spelled as `side_attr()` spells one in C.
pub const fn side_attr(key: &'static str, value: AttrValue) -> Attr {
    Attr { key, value }
}

pub const fn side_attr_null() -> AttrValue {
    AttrValue::Null
}

pub const fn side_attr_bool(value: bool) -> AttrValue {
    AttrValue::Bool(value)
}

pub const fn side_attr_u8(value: u8) -> AttrValue {
    AttrValue::U8(value)
}

pub const fn side_attr_u16(value: u16) -> AttrValue {
    AttrValue::U16(value)
}

pub const fn side_attr_u32(value: u32) -> AttrValue {
    AttrValue::U32(value)
}

pub const fn side_attr_u64(value: u64) -> AttrValue {
    AttrValue::U64(value)
}

pub const fn side_attr_s8(value: i8) -> AttrValue {
    AttrValue::S8(value)
}

pub const fn side_attr_s16(value: i16) -> AttrValue {
    AttrValue::S16(value)
}

pub const fn side_attr_s32(value: i32) -> AttrValue {
    AttrValue::S32(value)
}

pub const fn side_attr_s64(value: i64) -> AttrValue {
    AttrValue::S64(value)
}

pub const fn side_attr_string(value: &'static str) -> AttrValue {
    AttrValue::String(value)
}

/// A named field, of an event or of a gather structure.
#[derive(Clone, Copy)]
pub struct Field {
    pub name: &'static str,
    pub layout: Layout,
    /*
     * The attributes of the field's type, which is where libside keeps
     * them: side_field_u32("x", side_attr_list(...)) sets the ones of
     * the struct side_type_integer the field carries.
     */
    pub attributes: &'static [Attr],
}

/* Sizes and offsets, taken from the ABI declarations above. */

const DESC_SIZE: usize = size_of::<SideEventDescription>();
const FIELD_SIZE: usize = size_of::<SideEventField>();
const TYPE_SIZE: usize = size_of::<SideType>();
const STRUCT_TYPE_SIZE: usize = size_of::<SideTypeStruct>();
const ATTR_SIZE: usize = size_of::<SideAttr>();

const O_DESC_STRUCT_SIZE: usize = offset_of!(SideEventDescription, struct_size);
const O_DESC_VERSION: usize = offset_of!(SideEventDescription, version);
const O_DESC_PROVIDER: usize = offset_of!(SideEventDescription, provider_name);
const O_DESC_EVENT: usize = offset_of!(SideEventDescription, event_name);
const O_DESC_FIELDS: usize = offset_of!(SideEventDescription, fields);
const O_DESC_FLAGS: usize = offset_of!(SideEventDescription, flags);
const O_DESC_NR_TYPE: usize = offset_of!(SideEventDescription, nr_side_type_label);
const O_DESC_NR_ATTR: usize = offset_of!(SideEventDescription, nr_side_attr_type);
const O_DESC_LOGLEVEL: usize = offset_of!(SideEventDescription, loglevel);

const O_ARRAY_LENGTH: usize = offset_of!(SideRelArray, length);
const O_SEL_ARRAY_LENGTH: usize = offset_of!(SideSelArray, length);
const O_DESC_ATTRIBUTES: usize = offset_of!(SideEventDescription, attributes);

const O_ATTR_KEY: usize = offset_of!(SideAttr, key);
const O_ATTR_VALUE: usize = offset_of!(SideAttr, value);
const O_RAWSTR_P: usize = offset_of!(SideTypeRawString, p);
const O_RAWSTR_UNIT_SIZE: usize = offset_of!(SideTypeRawString, unit_size);
const O_RAWSTR_BYTE_ORDER: usize = offset_of!(SideTypeRawString, byte_order);
const O_ATTRVAL_TYPE: usize = offset_of!(SideAttrValue, type_);
const O_ATTRVAL_U: usize = offset_of!(SideAttrValue, u);

const O_BOOL_ATTRIBUTES: usize = offset_of!(SideTypeBool, attributes);
const O_INT_ATTRIBUTES: usize = offset_of!(SideTypeInteger, attributes);
const O_STR_ATTRIBUTES: usize = offset_of!(SideTypeString, attributes);
const O_ARRAY_ATTRIBUTES: usize = offset_of!(SideTypeArray, attributes);
const O_VLA_ATTRIBUTES: usize = offset_of!(SideTypeVla, attributes);
const O_SEL_IS_OFFSET: usize = offset_of!(SideSelPtr, is_offset);

const O_FIELD_NAME: usize = offset_of!(SideEventField, field_name);
const O_FIELD_TYPE: usize = offset_of!(SideEventField, side_type);
const O_TYPE_U: usize = offset_of!(SideType, u);

const O_BOOL_SIZE: usize = offset_of!(SideTypeBool, bool_size);
const O_BOOL_LEN_BITS: usize = offset_of!(SideTypeBool, len_bits);
const O_BOOL_BYTE_ORDER: usize = offset_of!(SideTypeBool, byte_order);

const O_INT_SIZE: usize = offset_of!(SideTypeInteger, integer_size);
const O_INT_LEN_BITS: usize = offset_of!(SideTypeInteger, len_bits);
const O_INT_SIGNEDNESS: usize = offset_of!(SideTypeInteger, signedness);
const O_INT_BYTE_ORDER: usize = offset_of!(SideTypeInteger, byte_order);

const O_STR_UNIT_SIZE: usize = offset_of!(SideTypeString, unit_size);
const O_STR_BYTE_ORDER: usize = offset_of!(SideTypeString, byte_order);

const O_GBOOL_OFFSET: usize = offset_of!(SideTypeGatherBool, offset);
const O_GBOOL_OFFSET_BITS: usize = offset_of!(SideTypeGatherBool, offset_bits);
const O_GBOOL_ACCESS: usize = offset_of!(SideTypeGatherBool, access_mode);
const O_GBOOL_TYPE: usize = offset_of!(SideTypeGatherBool, type_);

const O_GINT_OFFSET: usize = offset_of!(SideTypeGatherInteger, offset);
const O_GINT_OFFSET_BITS: usize = offset_of!(SideTypeGatherInteger, offset_bits);
const O_GINT_ACCESS: usize = offset_of!(SideTypeGatherInteger, access_mode);
const O_GINT_TYPE: usize = offset_of!(SideTypeGatherInteger, type_);

const O_GSTRUCT_TYPE: usize = offset_of!(SideTypeGatherStruct, type_);
const O_GSTRUCT_OFFSET: usize = offset_of!(SideTypeGatherStruct, offset);
const O_GSTRUCT_ACCESS: usize = offset_of!(SideTypeGatherStruct, access_mode);
const O_GSTRUCT_SIZE: usize = offset_of!(SideTypeGatherStruct, size);

const O_GARRAY_OFFSET: usize = offset_of!(SideTypeGatherArray, offset);
const O_GARRAY_ACCESS: usize = offset_of!(SideTypeGatherArray, access_mode);
const O_GARRAY_TYPE: usize = offset_of!(SideTypeGatherArray, type_);

const O_GVLA_OFFSET: usize = offset_of!(SideTypeGatherVla, offset);
const O_GVLA_ACCESS: usize = offset_of!(SideTypeGatherVla, access_mode);
const O_GVLA_TYPE: usize = offset_of!(SideTypeGatherVla, type_);

const O_ARRAY_ELEM_TYPE: usize = offset_of!(SideTypeArray, elem_type);
const O_ARRAY_TYPE_LENGTH: usize = offset_of!(SideTypeArray, length);

const O_VLA_ELEM_TYPE: usize = offset_of!(SideTypeVla, elem_type);
const O_VLA_LENGTH_TYPE: usize = offset_of!(SideTypeVla, length_type);

const O_STRUCT_FIELDS: usize = offset_of!(SideTypeStruct, fields);

/* Writing the description. */

const fn put_u8(buf: &mut [u8], at: usize, value: u8) {
    buf[at] = value;
}

const fn put_u16(buf: &mut [u8], at: usize, value: u16) {
    let bytes = value.to_ne_bytes();
    let mut i = 0;
    while i < bytes.len() {
        buf[at + i] = bytes[i];
        i += 1;
    }
}

const fn put_u32(buf: &mut [u8], at: usize, value: u32) {
    let bytes = value.to_ne_bytes();
    let mut i = 0;
    while i < bytes.len() {
        buf[at + i] = bytes[i];
        i += 1;
    }
}

const fn put_u64(buf: &mut [u8], at: usize, value: u64) {
    let bytes = value.to_ne_bytes();
    let mut i = 0;
    while i < bytes.len() {
        buf[at + i] = bytes[i];
        i += 1;
    }
}

/// Write, at byte `at`, the distance from there to byte `target`.
///
/// This is what `side_ptr_rel_get()` reads: it adds the value to the
/// address of the member holding it. Both ends are within one object,
/// so the distance is known here and nothing is left for the loader.
const fn put_rel(buf: &mut [u8], at: usize, target: usize) {
    put_u64(buf, at, (target as i64 - at as i64) as u64);
}

/// Write, at byte `at`, a selector pointer holding the distance from
/// there to byte `target`.
///
/// A member which refers to a type by name holds either a distance or
/// an address, with a byte beside them saying which; what this crate
/// builds is always within the one description, so it is a distance.
/// See `side_ptr_sel_t`.
const fn put_sel_rel(buf: &mut [u8], at: usize, target: usize) {
    put_rel(buf, at, target);
    put_u8(buf, at + O_SEL_IS_OFFSET, 1);
}

/// Write a nul terminated string at `pos`, and return where it starts
/// and where the next object goes.
const fn put_str(buf: &mut [u8], pos: usize, value: &str) -> (usize, usize) {
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        buf[pos + i] = bytes[i];
        i += 1;
    }
    buf[pos + bytes.len()] = 0;
    (pos, pos + bytes.len() + 1)
}

const fn put_bool_body(buf: &mut [u8], at: usize, size: u16) {
    put_u16(buf, at + O_BOOL_SIZE, size);
    put_u16(buf, at + O_BOOL_LEN_BITS, 0);
    put_u8(buf, at + O_BOOL_BYTE_ORDER, SIDE_TYPE_BYTE_ORDER_HOST);
}

const fn put_integer_body(buf: &mut [u8], at: usize, size: u16, signed: bool) {
    put_u16(buf, at + O_INT_SIZE, size);
    put_u16(buf, at + O_INT_LEN_BITS, 0);
    put_u8(buf, at + O_INT_SIGNEDNESS, signed as u8);
    put_u8(buf, at + O_INT_BYTE_ORDER, SIDE_TYPE_BYTE_ORDER_HOST);
}

const fn stack_integer_label(size: u16, signed: bool) -> u16 {
    match (size, signed) {
        (1, false) => SIDE_TYPE_U8,
        (2, false) => SIDE_TYPE_U16,
        (4, false) => SIDE_TYPE_U32,
        (8, false) => SIDE_TYPE_U64,
        (1, true) => SIDE_TYPE_S8,
        (2, true) => SIDE_TYPE_S16,
        (4, true) => SIDE_TYPE_S32,
        (8, true) => SIDE_TYPE_S64,
        _ => panic!("unsupported integer size"),
    }
}

/// One event, as a group lays it out.
#[derive(Clone, Copy)]
pub struct EventSpec {
    pub provider: &'static str,
    pub event: &'static str,
    pub loglevel: u32,
    pub flags: u64,
    pub fields: &'static [Field],
    pub attributes: &'static [Attr],
}

/*
 * Sharing a type between the events of a group.
 *
 * What a structure costs is its description -- the side_type_struct,
 * its field array, the names of those fields and every type they reach
 * -- and that is the same bytes wherever the structure is used. Only
 * the 64 byte side_type at the point of use differs, since it carries
 * the offset the value is read from and how to reach it.
 *
 * So a group lays a structure out once and every use of it holds the
 * distance to that one copy, which is what C does with
 * side_static_define_struct(). Two structures which describe the same
 * way ARE the same description, so this compares the shape rather than
 * asking which Rust type it came from: merging them is right, not a
 * coincidence to be avoided.
 */
const MAX_SHARED_TYPES: usize = 64;

struct Shared {
    fields: [&'static [Field]; MAX_SHARED_TYPES],
    size: [u32; MAX_SHARED_TYPES],
    offset: [usize; MAX_SHARED_TYPES],
    len: usize,
}

impl Shared {
    const fn new() -> Self {
        Self {
            fields: [&[]; MAX_SHARED_TYPES],
            size: [0; MAX_SHARED_TYPES],
            offset: [0; MAX_SHARED_TYPES],
            len: 0,
        }
    }
}

/// Where a structure of this shape was already laid out, if it was.
const fn shared_find(shared: &Shared, fields: &'static [Field], size: u32) -> Option<usize> {
    let mut i = 0;
    while i < shared.len {
        if shared.size[i] == size && fields_eq(shared.fields[i], fields) {
            return Some(shared.offset[i]);
        }
        i += 1;
    }
    None
}

/// Say where a structure of this shape is being laid out.
///
/// Recorded before its fields are, so that a structure which reaches
/// itself finds the copy already under way rather than laying out
/// another for ever.
const fn shared_insert(
    shared: &mut Shared,
    fields: &'static [Field],
    size: u32,
    offset: usize,
) {
    assert!(
        shared.len < MAX_SHARED_TYPES,
        "side: too many distinct structures in one group of events"
    );
    shared.fields[shared.len] = fields;
    shared.size[shared.len] = size;
    shared.offset[shared.len] = offset;
    shared.len += 1;
}

const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

const fn fields_eq(a: &[Field], b: &[Field]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if !str_eq(a[i].name, b[i].name)
            || !layout_eq(&a[i].layout, &b[i].layout)
            || !attrs_eq(a[i].attributes, b[i].attributes)
        {
            return false;
        }
        i += 1;
    }
    true
}

const fn attrs_eq(a: &[Attr], b: &[Attr]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if !str_eq(a[i].key, b[i].key) || !attr_value_eq(&a[i].value, &b[i].value) {
            return false;
        }
        i += 1;
    }
    true
}

const fn attr_value_eq(a: &AttrValue, b: &AttrValue) -> bool {
    match (*a, *b) {
        (AttrValue::Null, AttrValue::Null) => true,
        (AttrValue::Bool(x), AttrValue::Bool(y)) => x == y,
        (AttrValue::U8(x), AttrValue::U8(y)) => x == y,
        (AttrValue::U16(x), AttrValue::U16(y)) => x == y,
        (AttrValue::U32(x), AttrValue::U32(y)) => x == y,
        (AttrValue::U64(x), AttrValue::U64(y)) => x == y,
        (AttrValue::S8(x), AttrValue::S8(y)) => x == y,
        (AttrValue::S16(x), AttrValue::S16(y)) => x == y,
        (AttrValue::S32(x), AttrValue::S32(y)) => x == y,
        (AttrValue::S64(x), AttrValue::S64(y)) => x == y,
        (AttrValue::String(x), AttrValue::String(y)) => str_eq(x, y),
        _ => false,
    }
}

/// How many bytes an attribute list occupies: the array, the key of
/// each attribute, and any string one holds as its value.
const fn attrs_size(attrs: &[Attr]) -> usize {
    let mut size = attrs.len() * ATTR_SIZE;
    let mut i = 0;
    while i < attrs.len() {
        size += attrs[i].key.len() + 1;
        if let AttrValue::String(value) = attrs[i].value {
            size += value.len() + 1;
        }
        i += 1;
    }
    size
}

const fn layout_eq(a: &Layout, b: &Layout) -> bool {
    match *a {
        Layout::Bool => matches!(*b, Layout::Bool),
        Layout::Integer { size, signed } => match *b {
            Layout::Integer {
                size: bs,
                signed: bg,
            } => size == bs && signed == bg,
            _ => false,
        },
        Layout::Pointer => matches!(*b, Layout::Pointer),
        Layout::String => matches!(*b, Layout::String),
        Layout::GatherBool { offset, size } => match *b {
            Layout::GatherBool {
                offset: bo,
                size: bs,
            } => offset == bo && size == bs,
            _ => false,
        },
        Layout::GatherInteger {
            offset,
            size,
            signed,
        } => match *b {
            Layout::GatherInteger {
                offset: bo,
                size: bs,
                signed: bg,
            } => offset == bo && size == bs && signed == bg,
            _ => false,
        },
        Layout::GatherPointer { offset } => match *b {
            Layout::GatherPointer { offset: bo } => offset == bo,
            _ => false,
        },
        Layout::GatherStruct {
            offset,
            size,
            access,
            fields,
        } => match *b {
            Layout::GatherStruct {
                offset: bo,
                size: bs,
                access: ba,
                fields: bf,
            } => offset == bo && size == bs && access == ba && fields_eq(fields, bf),
            _ => false,
        },
        Layout::GatherArray {
            offset,
            length,
            elem,
        } => match *b {
            Layout::GatherArray {
                offset: bo,
                length: bl,
                elem: be,
            } => offset == bo && length == bl && layout_eq(elem, be),
            _ => false,
        },
        Layout::GatherVla {
            offset,
            len_offset,
            elem,
        } => match *b {
            Layout::GatherVla {
                offset: bo,
                len_offset: bl,
                elem: be,
            } => offset == bo && len_offset == bl && layout_eq(elem, be),
            _ => false,
        },
        Layout::ExternStruct {
            offset,
            size,
            access,
            target,
        } => match *b {
            Layout::ExternStruct {
                offset: bo,
                size: bs,
                access: ba,
                target: bt,
            } => offset == bo && size == bs && access == ba && target == bt,
            _ => false,
        },
    }
}

/// How many bytes a field list occupies: the array itself, the name of
/// each field, and whatever the type of each field hangs off itself.
const fn fields_size(fields: &'static [Field], shared: &mut Shared) -> usize {
    let mut size = fields.len() * FIELD_SIZE;
    let mut i = 0;
    while i < fields.len() {
        size += fields[i].name.len() + 1;
        size += attrs_size(fields[i].attributes);
        size += layout_size(&fields[i].layout, shared);
        i += 1;
    }
    size
}

/// How many bytes a type hangs off itself, beyond its own 64. A
/// structure the group has already laid out costs nothing more.
const fn layout_size(layout: &Layout, shared: &mut Shared) -> usize {
    match *layout {
        Layout::GatherStruct { size, fields, .. } => {
            if shared_find(shared, fields, size).is_some() {
                0
            } else {
                shared_insert(shared, fields, size, 0);
                STRUCT_TYPE_SIZE + fields_size(fields, shared)
            }
        }
        Layout::GatherArray { elem, .. } => TYPE_SIZE + layout_size(elem, shared),
        /* An element type and a length type. */
        Layout::GatherVla { elem, .. } => 2 * TYPE_SIZE + layout_size(elem, shared),
        /* Described in another object, so nothing is laid out here. */
        Layout::ExternStruct { .. } => 0,
        _ => 0,
    }
}

/// How many bytes the descriptions of a group of events occupy.
pub const fn group_size(events: &[EventSpec]) -> usize {
    let mut shared = Shared::new();
    let mut size = events.len() * DESC_SIZE;
    let mut i = 0;
    while i < events.len() {
        size += events[i].provider.len() + 1;
        size += events[i].event.len() + 1;
        size += attrs_size(events[i].attributes);
        size += fields_size(events[i].fields, &mut shared);
        i += 1;
    }
    size
}

/// Where the description of the event at `index` begins.
///
/// The descriptions come first and are all the same size, so this is
/// what a state needs to reach the one it belongs to.
pub const fn event_offset(index: usize) -> usize {
    index * DESC_SIZE
}

/// Lay out an attribute list at `pos` and point `at` at it, which is a
/// distance where the member holds one and a distance with the selector
/// byte set where it holds either. Nothing is written for an empty
/// list: the zeroed bytes it starts as are one.
const fn put_attrs(
    buf: &mut [u8],
    pos: usize,
    at: usize,
    selector: bool,
    attrs: &[Attr],
) -> usize {
    if attrs.is_empty() {
        return pos;
    }

    let array = pos;
    let mut pos = pos + attrs.len() * ATTR_SIZE;
    let mut i = 0;

    while i < attrs.len() {
        let attr = array + i * ATTR_SIZE;
        let key = attr + O_ATTR_KEY;
        let value = attr + O_ATTR_VALUE;

        let (at_key, next) = put_str(buf, pos, attrs[i].key);
        pos = next;
        put_raw_string(buf, key, at_key);

        match attrs[i].value {
            AttrValue::Null => put_u32(buf, value + O_ATTRVAL_TYPE, SIDE_ATTR_TYPE_NULL),
            AttrValue::Bool(v) => {
                put_u32(buf, value + O_ATTRVAL_TYPE, SIDE_ATTR_TYPE_BOOL);
                put_u8(buf, value + O_ATTRVAL_U, v as u8);
            }
            AttrValue::U8(v) => {
                put_u32(buf, value + O_ATTRVAL_TYPE, SIDE_ATTR_TYPE_U8);
                put_u8(buf, value + O_ATTRVAL_U, v);
            }
            AttrValue::U16(v) => {
                put_u32(buf, value + O_ATTRVAL_TYPE, SIDE_ATTR_TYPE_U16);
                put_u16(buf, value + O_ATTRVAL_U, v);
            }
            AttrValue::U32(v) => {
                put_u32(buf, value + O_ATTRVAL_TYPE, SIDE_ATTR_TYPE_U32);
                put_u32(buf, value + O_ATTRVAL_U, v);
            }
            AttrValue::U64(v) => {
                put_u32(buf, value + O_ATTRVAL_TYPE, SIDE_ATTR_TYPE_U64);
                put_u64(buf, value + O_ATTRVAL_U, v);
            }
            AttrValue::S8(v) => {
                put_u32(buf, value + O_ATTRVAL_TYPE, SIDE_ATTR_TYPE_S8);
                put_u8(buf, value + O_ATTRVAL_U, v as u8);
            }
            AttrValue::S16(v) => {
                put_u32(buf, value + O_ATTRVAL_TYPE, SIDE_ATTR_TYPE_S16);
                put_u16(buf, value + O_ATTRVAL_U, v as u16);
            }
            AttrValue::S32(v) => {
                put_u32(buf, value + O_ATTRVAL_TYPE, SIDE_ATTR_TYPE_S32);
                put_u32(buf, value + O_ATTRVAL_U, v as u32);
            }
            AttrValue::S64(v) => {
                put_u32(buf, value + O_ATTRVAL_TYPE, SIDE_ATTR_TYPE_S64);
                put_u64(buf, value + O_ATTRVAL_U, v as u64);
            }
            AttrValue::String(v) => {
                put_u32(buf, value + O_ATTRVAL_TYPE, SIDE_ATTR_TYPE_STRING);
                let (at_value, next) = put_str(buf, pos, v);
                pos = next;
                put_raw_string(buf, value + O_ATTRVAL_U, at_value);
            }
        }
        i += 1;
    }

    if selector {
        put_sel_rel(buf, at, array);
        put_u32(buf, at + O_SEL_ARRAY_LENGTH, attrs.len() as u32);
    } else {
        put_rel(buf, at, array);
        put_u32(buf, at + O_ARRAY_LENGTH, attrs.len() as u32);
    }
    pos
}

/// A `struct side_type_raw_string` at `at`, holding the string at
/// `target`. Both a key and a string value are one.
const fn put_raw_string(buf: &mut [u8], at: usize, target: usize) {
    put_sel_rel(buf, at + O_RAWSTR_P, target);
    put_u8(buf, at + O_RAWSTR_UNIT_SIZE, size_of::<u8>() as u8);
    put_u8(buf, at + O_RAWSTR_BYTE_ORDER, SIDE_TYPE_BYTE_ORDER_HOST);
}

/// Lay out a field array at `pos`, and return where it starts and where
/// the next object goes.
const fn put_fields<const K: usize>(
    buf: &mut [u8],
    pos: usize,
    fields: &'static [Field],
    shared: &mut Shared,
    patches: &mut Patches<K>,
) -> (usize, usize) {
    let array = pos;
    let mut pos = pos + fields.len() * FIELD_SIZE;
    let mut i = 0;
    while i < fields.len() {
        let field = array + i * FIELD_SIZE;
        let (name, next) = put_str(buf, pos, fields[i].name);
        pos = next;
        put_rel(buf, field + O_FIELD_NAME, name);
        pos = put_type(
            buf,
            pos,
            field + O_FIELD_TYPE,
            &fields[i].layout,
            fields[i].attributes,
            shared,
            patches,
        );
        i += 1;
    }
    (array, pos)
}

/// Write a `struct side_type` at `at`, putting whatever it points at at
/// `pos`, and return where the next object goes.
const fn put_type<const K: usize>(
    buf: &mut [u8],
    pos: usize,
    at: usize,
    layout: &Layout,
    attributes: &'static [Attr],
    shared: &mut Shared,
    patches: &mut Patches<K>,
) -> usize {
    let u = at + O_TYPE_U;
    let mut pos = pos;

    match *layout {
        Layout::Bool => {
            put_u16(buf, at, SIDE_TYPE_BOOL);
            put_bool_body(buf, u, size_of::<u8>() as u16);
            pos = put_attrs(buf, pos, u + O_BOOL_ATTRIBUTES, true, attributes);
        }
        Layout::Integer { size, signed } => {
            put_u16(buf, at, stack_integer_label(size, signed));
            put_integer_body(buf, u, size, signed);
            pos = put_attrs(buf, pos, u + O_INT_ATTRIBUTES, true, attributes);
        }
        Layout::Pointer => {
            put_u16(buf, at, SIDE_TYPE_POINTER);
            put_integer_body(buf, u, size_of::<usize>() as u16, false);
            pos = put_attrs(buf, pos, u + O_INT_ATTRIBUTES, true, attributes);
        }
        Layout::String => {
            put_u16(buf, at, SIDE_TYPE_STRING_UTF8);
            put_u8(buf, u + O_STR_UNIT_SIZE, size_of::<u8>() as u8);
            put_u8(buf, u + O_STR_BYTE_ORDER, SIDE_TYPE_BYTE_ORDER_HOST);
            pos = put_attrs(buf, pos, u + O_STR_ATTRIBUTES, true, attributes);
        }
        Layout::GatherBool { offset, size } => {
            put_u16(buf, at, SIDE_TYPE_GATHER_BOOL);
            put_u64(buf, u + O_GBOOL_OFFSET, offset);
            put_u16(buf, u + O_GBOOL_OFFSET_BITS, 0);
            put_u8(buf, u + O_GBOOL_ACCESS, SIDE_TYPE_GATHER_ACCESS_DIRECT);
            put_bool_body(buf, u + O_GBOOL_TYPE, size);
            pos = put_attrs(buf, pos, u + O_GBOOL_TYPE + O_BOOL_ATTRIBUTES, true, attributes);
        }
        Layout::GatherInteger {
            offset,
            size,
            signed,
        } => {
            put_u16(buf, at, SIDE_TYPE_GATHER_INTEGER);
            put_u64(buf, u + O_GINT_OFFSET, offset);
            put_u16(buf, u + O_GINT_OFFSET_BITS, 0);
            put_u8(buf, u + O_GINT_ACCESS, SIDE_TYPE_GATHER_ACCESS_DIRECT);
            put_integer_body(buf, u + O_GINT_TYPE, size, signed);
            pos = put_attrs(buf, pos, u + O_GINT_TYPE + O_INT_ATTRIBUTES, true, attributes);
        }
        Layout::GatherPointer { offset } => {
            put_u16(buf, at, SIDE_TYPE_GATHER_POINTER);
            put_u64(buf, u + O_GINT_OFFSET, offset);
            put_u16(buf, u + O_GINT_OFFSET_BITS, 0);
            put_u8(buf, u + O_GINT_ACCESS, SIDE_TYPE_GATHER_ACCESS_DIRECT);
            put_integer_body(buf, u + O_GINT_TYPE, size_of::<usize>() as u16, false);
            pos = put_attrs(buf, pos, u + O_GINT_TYPE + O_INT_ATTRIBUTES, true, attributes);
        }
        Layout::GatherStruct {
            offset,
            size,
            access,
            fields,
        } => {
            /*
             * A structure keeps its attributes with its definition, as
             * it does in C: side_field_gather_struct() takes none.
             */
            assert!(
                attributes.is_empty(),
                "side: the attributes of a structure belong to the structure itself"
            );
            put_u16(buf, at, SIDE_TYPE_GATHER_STRUCT);
            put_u64(buf, u + O_GSTRUCT_OFFSET, offset);
            put_u8(buf, u + O_GSTRUCT_ACCESS, access);
            put_u32(buf, u + O_GSTRUCT_SIZE, size);

            /* One copy of the structure for the whole group. */
            let type_ = match shared_find(shared, fields, size) {
                Some(offset) => offset,
                None => {
                    let type_ = pos;
                    pos += STRUCT_TYPE_SIZE;
                    shared_insert(shared, fields, size, type_);

                    let (array, next) = put_fields(buf, pos, fields, shared, patches);
                    pos = next;
                    put_rel(buf, type_ + O_STRUCT_FIELDS, array);
                    put_u32(
                        buf,
                        type_ + O_STRUCT_FIELDS + O_ARRAY_LENGTH,
                        fields.len() as u32,
                    );
                    type_
                }
            };
            put_sel_rel(buf, u + O_GSTRUCT_TYPE, type_);
        }
        Layout::GatherArray {
            offset,
            length,
            elem,
        } => {
            put_u16(buf, at, SIDE_TYPE_GATHER_ARRAY);
            put_u64(buf, u + O_GARRAY_OFFSET, offset);
            put_u8(buf, u + O_GARRAY_ACCESS, SIDE_TYPE_GATHER_ACCESS_DIRECT);

            let array = u + O_GARRAY_TYPE;
            put_u32(buf, array + O_ARRAY_TYPE_LENGTH, length);
            pos = put_attrs(buf, pos, array + O_ARRAY_ATTRIBUTES, false, attributes);

            let elem_type = pos;
            pos += TYPE_SIZE;
            put_rel(buf, array + O_ARRAY_ELEM_TYPE, elem_type);
            pos = put_type(buf, pos, elem_type, elem, &[], shared, patches);
        }
        Layout::GatherVla {
            offset,
            len_offset,
            elem,
        } => {
            put_u16(buf, at, SIDE_TYPE_GATHER_VLA);
            put_u64(buf, u + O_GVLA_OFFSET, offset);
            /* The offset names a pointer, which is dereferenced. */
            put_u8(buf, u + O_GVLA_ACCESS, SIDE_TYPE_GATHER_ACCESS_POINTER);

            let vla = u + O_GVLA_TYPE;
            pos = put_attrs(buf, pos, vla + O_VLA_ATTRIBUTES, false, attributes);

            let elem_type = pos;
            pos += TYPE_SIZE;
            let length_type = pos;
            pos += TYPE_SIZE;
            put_rel(buf, vla + O_VLA_ELEM_TYPE, elem_type);
            put_rel(buf, vla + O_VLA_LENGTH_TYPE, length_type);

            pos = put_type(buf, pos, elem_type, elem, &[], shared, patches);
            /* The length is read from the vector itself. */
            pos = put_type(
                buf,
                pos,
                length_type,
                &Layout::GatherInteger {
                    offset: len_offset,
                    size: size_of::<usize>() as u16,
                    signed: false,
                },
                &[],
                shared,
                patches,
            );
        }
        Layout::ExternStruct {
            offset,
            size,
            access,
            /* Which structure is the loader's business, not ours. */
            target: _,
        } => {
            /*
             * A structure keeps its attributes with its definition, as
             * it does in C: side_field_gather_struct() takes none.
             */
            assert!(
                attributes.is_empty(),
                "side: the attributes of a structure belong to the structure itself"
            );
            put_u16(buf, at, SIDE_TYPE_GATHER_STRUCT);
            put_u64(buf, u + O_GSTRUCT_OFFSET, offset);
            put_u8(buf, u + O_GSTRUCT_ACCESS, access);
            put_u32(buf, u + O_GSTRUCT_SIZE, size);
            /*
             * The selector byte is already zero, which says the
             * reference holds an address, and the address itself is a
             * hole the object built around these bytes fills with a
             * pointer. See `Patch'.
             */
            patch(patches, u + O_GSTRUCT_TYPE);
        }
    }

    pos
}

/// The descriptions of a group of events, as the bytes which go in the
/// `side_event_description` section.
///
/// The descriptions come first, one after another, so the description
/// of the event at index `i` begins at `event_offset(i)`; everything
/// they reach follows, with each distinct structure laid out once.
///
/// `N` must be `group_size(events)`; the assertion at the end says so
/// if the two ever disagree.
pub const fn build_group<const N: usize, const K: usize>(events: &[EventSpec]) -> Built<N, K> {
    let mut buf = [0u8; N];
    let mut shared = Shared::new();
    let mut patches = Patches::<K>::new();
    let mut pos = events.len() * DESC_SIZE;
    let mut e = 0;

    while e < events.len() {
        let desc = event_offset(e);
        let event = events[e];

        put_u32(&mut buf, desc + O_DESC_STRUCT_SIZE, DESC_SIZE as u32);
        put_u32(
            &mut buf,
            desc + O_DESC_VERSION,
            SIDE_EVENT_DESCRIPTION_ABI_VERSION,
        );
        put_u64(&mut buf, desc + O_DESC_FLAGS, event.flags);
        put_u16(&mut buf, desc + O_DESC_NR_TYPE, SIDE_NR_TYPE_LABEL);
        put_u16(&mut buf, desc + O_DESC_NR_ATTR, SIDE_NR_ATTR_TYPE);
        put_u32(&mut buf, desc + O_DESC_LOGLEVEL, event.loglevel);

        let (name, next) = put_str(&mut buf, pos, event.provider);
        pos = next;
        put_rel(&mut buf, desc + O_DESC_PROVIDER, name);

        let (name, next) = put_str(&mut buf, pos, event.event);
        pos = next;
        put_rel(&mut buf, desc + O_DESC_EVENT, name);

        let (array, next) = put_fields(&mut buf, pos, event.fields, &mut shared, &mut patches);
        pos = next;
        put_rel(&mut buf, desc + O_DESC_FIELDS, array);
        put_u32(
            &mut buf,
            desc + O_DESC_FIELDS + O_ARRAY_LENGTH,
            event.fields.len() as u32,
        );

        /*
         * An empty attribute list is left as the zeroed bytes it starts
         * as: a length of zero, which is what a reader looks at before
         * the pointer, and a pointer which is a distance of zero where
         * the member holds one and a null address where it holds
         * either. Nothing follows it.
         */
        pos = put_attrs(&mut buf, pos, desc + O_DESC_ATTRIBUTES, false, event.attributes);
        e += 1;
    }

    assert!(pos == N, "the descriptions were not laid out as measured");
    assert!(
        patches.len == K,
        "the references to a structure described elsewhere were not as counted"
    );
    Built {
        bytes: buf,
        patches: patches.at,
    }
}

/// How many bytes the description of one structure occupies, on its own.
///
/// This is what `define_type!` lays out: a structure another object can
/// refer to by address, which is how a description crosses a boundary
/// the const evaluator cannot measure across.
pub const fn type_size(fields: &'static [Field], size: u32) -> usize {
    let mut shared = Shared::new();
    /* Named before its fields are laid out, as a use of it would be. */
    shared_insert(&mut shared, fields, size, 0);
    STRUCT_TYPE_SIZE + fields_size(fields, &mut shared)
}

/// The description of one structure, as the bytes which go in the
/// `side_event_description` section. The `struct side_type_struct` an
/// address reaches is the first thing in it.
pub const fn build_type<const N: usize, const K: usize>(
    fields: &'static [Field],
    size: u32,
) -> Built<N, K> {
    let mut buf = [0u8; N];
    let mut shared = Shared::new();
    let mut patches = Patches::<K>::new();

    shared_insert(&mut shared, fields, size, 0);
    let (array, pos) = put_fields(&mut buf, STRUCT_TYPE_SIZE, fields, &mut shared, &mut patches);
    put_rel(&mut buf, O_STRUCT_FIELDS, array);
    put_u32(
        &mut buf,
        O_STRUCT_FIELDS + O_ARRAY_LENGTH,
        fields.len() as u32,
    );

    assert!(pos == N, "the structure was not laid out as measured");
    assert!(
        patches.len == K,
        "the references to a structure described elsewhere were not as counted"
    );
    Built {
        bytes: buf,
        patches: patches.at,
    }
}

pub struct PreparedSideArg {
    arg: SideArg,
    _owned_string: Option<CString>,
    _owned_gather: Option<Box<dyn Any>>,
}

impl PreparedSideArg {
    #[doc(hidden)]
    pub fn new(arg: SideArg) -> Self {
        Self {
            arg,
            _owned_string: None,
            _owned_gather: None,
        }
    }

    fn string(value: &CStr) -> Self {
        Self::new(SideArg::string(value))
    }

    fn owned_string(value: CString) -> Self {
        let arg = SideArg::string(value.as_c_str());
        Self {
            arg,
            _owned_string: Some(value),
            _owned_gather: None,
        }
    }

    #[doc(hidden)]
    pub fn owned_gather<T: 'static>(value: T) -> Self {
        let value = Box::new(value);
        let arg = SideArg::gather_struct(value.as_ref() as *const T);
        Self {
            arg,
            _owned_string: None,
            _owned_gather: Some(value),
        }
    }

    pub fn as_side_arg(&self) -> SideArg {
        self.arg
    }
}

pub trait FieldType: Sized {
    const LAYOUT: Layout;

    fn into_prepared_arg(self) -> PreparedSideArg;

    fn with_side_arg<R>(self, f: impl FnOnce(SideArg) -> R) -> R {
        let prepared = self.into_prepared_arg();
        f(prepared.as_side_arg())
    }
}

/// A Rust structure libside reads members out of.
///
/// The derive names the members; the two constants below are what a
/// field site needs to describe the structure wherever it appears.
#[doc(hidden)]
pub trait GatherType {
    const FIELDS: &'static [Field];
    const SIZE: u32;

    /// The structure as the element of an array or of a vector, where
    /// the base address is the element itself.
    const ELEMENT: &'static Layout = &Layout::GatherStruct {
        offset: 0,
        size: Self::SIZE,
        access: SIDE_TYPE_GATHER_ACCESS_DIRECT,
        fields: Self::FIELDS,
    };
}

impl FieldType for bool {
    const LAYOUT: Layout = Layout::Bool;

    fn into_prepared_arg(self) -> PreparedSideArg {
        PreparedSideArg::new(SideArg::bool(self))
    }
}

macro_rules! impl_integer_field_type {
    ($ty:ty, $signed:expr, $arg:ident) => {
        impl FieldType for $ty {
            const LAYOUT: Layout = Layout::Integer {
                size: size_of::<$ty>() as u16,
                signed: $signed,
            };

            fn into_prepared_arg(self) -> PreparedSideArg {
                PreparedSideArg::new(SideArg::$arg(self))
            }
        }
    };
}

impl_integer_field_type!(u8, false, u8);
impl_integer_field_type!(u16, false, u16);
impl_integer_field_type!(u32, false, u32);
impl_integer_field_type!(u64, false, u64);
impl_integer_field_type!(i8, true, s8);
impl_integer_field_type!(i16, true, s16);
impl_integer_field_type!(i32, true, s32);
impl_integer_field_type!(i64, true, s64);

impl<T> FieldType for *const T {
    const LAYOUT: Layout = Layout::Pointer;

    fn into_prepared_arg(self) -> PreparedSideArg {
        PreparedSideArg::new(SideArg::pointer(self))
    }
}

impl<T> FieldType for *mut T {
    const LAYOUT: Layout = Layout::Pointer;

    fn into_prepared_arg(self) -> PreparedSideArg {
        PreparedSideArg::new(SideArg::pointer(self.cast_const()))
    }
}

impl<'a> FieldType for &'a CStr {
    const LAYOUT: Layout = Layout::String;

    fn into_prepared_arg(self) -> PreparedSideArg {
        PreparedSideArg::string(self)
    }
}

impl<'a> FieldType for &'a str {
    const LAYOUT: Layout = Layout::String;

    fn into_prepared_arg(self) -> PreparedSideArg {
        PreparedSideArg::owned_string(
            CString::new(self).expect("string fields must not contain interior NUL bytes"),
        )
    }
}

const _: [(); 16] = [(); size_of::<SideRawPtr>()];
const _: [(); 8] = [(); size_of::<SideRelPtr>()];
const _: [(); 12] = [(); size_of::<SideRelArray>()];
const _: [(); 17] = [(); size_of::<SideSelPtr>()];
const _: [(); 21] = [(); size_of::<SideSelArray>()];
const _: [(); 32] = [(); size_of::<SideBoolValue>()];
const _: [(); 32] = [(); size_of::<SideIntegerValue>()];
const _: [(); 32] = [(); size_of::<SideArgStatic>()];
const _: [(); 60] = [(); size_of::<SideArgPayload>()];
const _: [(); 64] = [(); size_of::<SideArg>()];
const _: [(); 20] = [(); size_of::<SideArgVec>()];
const _: [(); 19] = [(); size_of::<SideTypeRawString>()];
const _: [(); 32] = [(); size_of::<SideAttrValuePayload>()];
const _: [(); 36] = [(); size_of::<SideAttrValue>()];
const _: [(); 55] = [(); size_of::<SideAttr>()];
const _: [(); 21] = [(); size_of::<SideTypeNull>()];
const _: [(); 26] = [(); size_of::<SideTypeBool>()];
const _: [(); 21] = [(); size_of::<SideTypeByte>()];
const _: [(); 23] = [(); size_of::<SideTypeString>()];
const _: [(); 27] = [(); size_of::<SideTypeInteger>()];
const _: [(); 24] = [(); size_of::<SideTypeStruct>()];
const _: [(); 24] = [(); size_of::<SideTypeArray>()];
const _: [(); 28] = [(); size_of::<SideTypeVla>()];
const _: [(); 37] = [(); size_of::<SideTypeGatherBool>()];
const _: [(); 38] = [(); size_of::<SideTypeGatherInteger>()];
const _: [(); 30] = [(); size_of::<SideTypeGatherStruct>()];
const _: [(); 33] = [(); size_of::<SideTypeGatherArray>()];
const _: [(); 37] = [(); size_of::<SideTypeGatherVla>()];
const _: [(); 61] = [(); size_of::<SideTypeGather>()];
const _: [(); 64] = [(); size_of::<SideType>()];
const _: [(); 72] = [(); size_of::<SideEventField>()];
const _: [(); 8 + 3 * size_of::<*const c_void>()] = [(); size_of::<SideEventState0>()];
const _: [(); 64] = [(); size_of::<SideEventDescription>()];

/// Emit an event of a group, asking whether it is enabled before its
/// arguments are worked out.
///
/// A group gives each of its events a function, which is the plain way
/// to emit one; but the arguments of a function are evaluated before it
/// is entered, including when nothing is listening. This asks first, so
/// an argument which costs something, or which has an effect of its
/// own, is not reached at all while the event is disabled:
///
/// ```ignore
/// side_event!(trace::request, id, render(&body));
/// ```
///
/// The path is written here and resolves here, which is what lets one
/// macro serve every event: `trace::request`, `crate::trace::request`,
/// or `request` where it has been brought in with `use`.
#[macro_export]
macro_rules! side_event {
    ($($path:ident)::+ $(, $arg:expr)* $(,)?) => {{
        if $($path)::+::enabled() {
            /*
             * Everything past here is the unlikely half. Saying so is
             * what moves it off the straight line a program which is
             * not being traced runs down, and it stays inlined, which a
             * cold function of its own would not: that would cost a
             * call every time the event *is* enabled.
             */
            ::core::hint::cold_path();
            $($path)::+::emit($($arg),*);
        }
    }};
}

/// Describe a structure in an object of its own.
///
/// Events laid out in another object cannot reach a description by a
/// distance -- a distance is between two bytes of one object -- so they
/// reach this one by address, which is what `side_extern(NAME)` says at
/// a field of a group. It is the same division of work as
/// `side_define_struct()` and `side_extern()` in the C API.
///
/// ```ignore
/// libside::define_type!(PROCESS_INFO, ProcessInfo);
/// ```
#[macro_export]
macro_rules! define_type {
    ($name:ident, $ty:ty $(,)?) => {
        #[allow(non_snake_case)]
        pub mod $name {
            use super::*;

            /// The Rust type described here, for the argument of a field.
            pub type Type = $ty;

            pub const FIELDS: &[$crate::side::Field] =
                <$ty as $crate::side::GatherType>::FIELDS;
            pub const SIZE: u32 = <$ty as $crate::side::GatherType>::SIZE;

            const LEN: usize = $crate::side::type_size(FIELDS, SIZE);
            /*
             * A structure described on its own holds no address of its
             * own: everything it reaches is within it, so there is
             * nothing for a constructor to write here, which is what
             * the zero says.
             */
            const BUILT: $crate::side::Built<LEN, 0> =
                $crate::side::build_type::<LEN, 0>(FIELDS, SIZE);

            #[repr(C, align(16))]
            pub struct TypeDesc(pub [u8; LEN]);

            #[used]
            #[cfg_attr(any(target_os = "linux", target_os = "android"), link_section = "side_event_description")]
            pub static mut DESC: TypeDesc = TypeDesc(BUILT.bytes);
        }
    };
}

#[macro_export]
macro_rules! __emit_event_call_macro {
    ($d:tt $name:ident, $module:ident, $first:ident $(, $field:ident)*) => {
        #[allow(unused_macros)]
        macro_rules! $name {
            ($d first_value:expr, $d($d field:ident : $d value:expr),* $d(,)?) => {{
                if unsafe { $module::enabled() } {
                    ::core::hint::cold_path();
                    let arguments = $module::__EventArguments {
                        $first: $d first_value,
                        $d($d field: $d value),*
                    };
                    $module::function(arguments.$first $(, arguments.$field)*);
                }
            }};
            ($d($d value:expr),* $d(,)?) => {{
                if unsafe { $module::enabled() } {
                    ::core::hint::cold_path();
                    $module::function($d($d value),*);
                }
            }};
        }
    };
}

#[macro_export]
macro_rules! define_event {
    (
        $name:ident,
        provider: $provider:literal,
        event: $event:literal,
        level: $level:expr,
        fields: (
            $( $arg:ident : $ty:ty $( [ $( $fattr:expr ),* $(,)? ] )? ),* $(,)?
        )
        $(, attributes: [ $( $eattr:expr ),* $(,)? ] )?
        $(,)?
    ) => {
        #[doc(hidden)]
        #[allow(non_snake_case)]
        mod $name {
            use super::*;

            pub type Type = fn($( $ty ),*);

            #[allow(non_camel_case_types)]
            pub(crate) struct __EventArguments<$( $arg ),*> {
                $( pub(crate) $arg: $arg ),*
            }

            const PROVIDER_NAME: &str = $provider;
            const EVENT_NAME: &str = $event;

            const EVENT_FIELDS: &[$crate::side::Field] = &[
                $(
                    $crate::side::Field {
                        name: ::core::stringify!($arg),
                        layout: <$ty as $crate::side::FieldType>::LAYOUT,
                        attributes: &[ $( $( $fattr ),* )? ],
                    }
                ),*
            ];

            /* One event is a group of one. */
            const EVENT_SPECS: &[$crate::side::EventSpec] = &[$crate::side::EventSpec {
                provider: PROVIDER_NAME,
                event: EVENT_NAME,
                loglevel: $level,
                flags: 0,
                fields: EVENT_FIELDS,
                attributes: &[ $( $( $eattr ),* )? ],
            }];

            const EVENT_DESC_SIZE: usize = $crate::side::group_size(EVENT_SPECS);

            /*
             * The whole description in one object: its name, its
             * fields, and every type they reach. Everything it points
             * at, it points at by a distance from the member holding
             * the pointer, so the object needs no relocation and the
             * pages it lives on stay clean and shared between
             * processes.
             */
            #[repr(C, align(16))]
            pub struct EventDesc([u8; EVENT_DESC_SIZE]);

            /*
             * A single event refers to nothing described elsewhere;
             * side_extern() is for a group. See side::Patch.
             */
            const EVENT_BUILT: $crate::side::Built<EVENT_DESC_SIZE, 0> =
                $crate::side::build_group::<EVENT_DESC_SIZE, 0>(EVENT_SPECS);

            #[used]
            #[cfg_attr(any(target_os = "linux", target_os = "android"), link_section = "side_event_description")]
            pub static mut EVENT_DESC: EventDesc = EventDesc(EVENT_BUILT.bytes);

            /*
             * The state is in a section of its own, because a tracer
             * writes to it when it enables the event. It is what holds
             * the address of the description, rather than the reverse.
             */
            #[used]
            #[cfg_attr(any(target_os = "linux", target_os = "android"), link_section = "side_event_state")]
            pub static mut EVENT_STATE: $crate::side::SideEventState0 =
                $crate::side::SideEventState0 {
                    parent: $crate::side::SideEventState {
                        version: $crate::side::SIDE_EVENT_STATE_ABI_VERSION,
                    },
                    nr_callbacks: 0,
                    enabled: ::core::sync::atomic::AtomicUsize::new(0),
                    callbacks: ::core::ptr::addr_of!($crate::side::side_empty_callback).cast(),
                    desc: ::core::ptr::addr_of_mut!(EVENT_DESC)
                        .cast::<$crate::side::SideEventDescription>(),
                };

            /* An event is reached by its state. */
            #[used]
            #[cfg_attr(any(target_os = "linux", target_os = "android"), link_section = "side_event_state_ptr")]
            static mut EVENT_STATE_PTR: *mut $crate::side::SideEventState =
                ::core::ptr::addr_of_mut!(EVENT_STATE).cast::<$crate::side::SideEventState>();

            static mut EVENT_REGISTER_HANDLE: *mut $crate::side::SideEventsRegisterHandle =
                ::core::ptr::null_mut();

            unsafe extern "C" fn register_event() {
                unsafe {
                    EVENT_REGISTER_HANDLE = $crate::side::side_events_register(
                        ::core::ptr::addr_of_mut!(EVENT_STATE_PTR),
                        1,
                    );
                }
            }

            unsafe extern "C" fn unregister_event() {
                unsafe {
                    $crate::side::side_events_unregister(EVENT_REGISTER_HANDLE);
                }
            }

            #[used]
            #[cfg_attr(any(target_os = "linux", target_os = "android"), link_section = ".init_array")]
            static EVENT_REGISTER_INIT: unsafe extern "C" fn() = register_event;

            #[used]
            #[cfg_attr(any(target_os = "linux", target_os = "android"), link_section = ".fini_array")]
            static EVENT_REGISTER_FINI: unsafe extern "C" fn() = unregister_event;

            pub(crate) unsafe fn enabled() -> bool {
                /*
                 * Relaxed, which is what side_event_enabled() reads it
                 * with: nothing is ordered against it, and the only
                 * thing asked of the compiler is that it read it here
                 * rather than remember what it held.
                 */
                let enabled = unsafe { &(*::core::ptr::addr_of_mut!(EVENT_STATE)).enabled };
                enabled.load(::core::sync::atomic::Ordering::Relaxed) != 0
            }

            unsafe fn state() -> *const $crate::side::SideEventState {
                ::core::ptr::addr_of!((*::core::ptr::addr_of_mut!(EVENT_STATE)).parent)
            }

            #[inline(always)]
            pub fn function( $( $arg : $ty ),* ) {
                $crate::define_event!(@with_side_args $name; [$( $arg ),*]; [$( $arg : $ty ),*]);
            }
        }

        macro_rules! __with_dollar {
            ($d:tt) => {
                $crate::__emit_event_call_macro!($d $name, $name, $( $arg ),*);
            };
        }
        __with_dollar!($);
    };

    (@with_side_args $module:ident; [$($name:ident),*]; []) => {
        unsafe {
            $crate::side::call($module::state(), &[$($name),*]);
        }
    };
    (@with_side_args $module:ident; [$($all:ident),*]; [$head:ident : $head_ty:ty $(, $tail:ident : $tail_ty:ty)*]) => {
        <$head_ty as $crate::side::FieldType>::with_side_arg($head, |$head| {
            $crate::define_event!(@with_side_args $module; [$($all),*]; [$($tail : $tail_ty),*])
        })
    };
}
