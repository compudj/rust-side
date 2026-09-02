extern crate alloc;

use alloc::boxed::Box;
use alloc::ffi::CString;
use core::any::Any;
use core::ffi::{c_char, c_void, CStr};
use core::mem::size_of;
use core::ptr::null;

#[cfg(not(any(target_pointer_width = "32", target_pointer_width = "64")))]
compile_error!("libside-rust currently supports 32-bit and 64-bit targets only");

const SIDE_PTR_WORDS: usize = 16 / size_of::<*const c_void>();

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

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideArray {
    pub elements: SideRawPtr,
    pub length: u32,
}

impl SideArray {
    pub const fn empty() -> Self {
        Self {
            elements: SideRawPtr::null(),
            length: 0,
        }
    }

    pub const fn new(elements: *const c_void, length: u32) -> Self {
        Self {
            elements: SideRawPtr::from_ptr(elements),
            length,
        }
    }
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

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideTypeNull {
    pub attributes: SideArray,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideTypeBool {
    pub attributes: SideArray,
    pub bool_size: u16,
    pub len_bits: u16,
    pub byte_order: u8,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideTypeByte {
    pub attributes: SideArray,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideTypeString {
    pub attributes: SideArray,
    pub unit_size: u8,
    pub byte_order: u8,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideTypeInteger {
    pub attributes: SideArray,
    pub integer_size: u16,
    pub len_bits: u16,
    pub signedness: u8,
    pub byte_order: u8,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideTypeStruct {
    pub fields: SideArray,
    pub attributes: SideArray,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideTypeArray {
    pub elem_type: SideRawPtr,
    pub length: u32,
    pub attributes: SideArray,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideTypeVla {
    pub elem_type: SideRawPtr,
    pub length_type: SideRawPtr,
    pub attributes: SideArray,
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
    pub type_: SideRawPtr,
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
    pub field_name: SideRawPtr,
    pub side_type: SideType,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SideEventState {
    pub version: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SideEventState0 {
    pub parent: SideEventState,
    pub nr_callbacks: u32,
    pub enabled: usize,
    pub callbacks: *const c_void,
    pub desc: *const SideEventDescription,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SideEventDescription {
    pub struct_size: u32,
    pub version: u32,
    pub state: SideRawPtr,
    pub provider_name: SideRawPtr,
    pub event_name: SideRawPtr,
    pub fields: SideArray,
    pub attributes: SideArray,
    pub flags: u64,
    pub nr_side_type_label: u16,
    pub nr_side_attr_type: u16,
    pub loglevel: u32,
}

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct SideEventDescriptionPtr(pub *const SideEventDescription);

unsafe impl Sync for SideEventDescriptionPtr {}

#[repr(C)]
pub struct SideEventsRegisterHandle {
    _private: [u8; 0],
}

extern "C" {
    pub fn side_call(state: *const SideEventState, side_arg_vec: *const SideArgVec);
    pub fn side_events_register(
        events: *mut *mut SideEventDescription,
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
    const FIELD_TYPE: SideType;

    fn into_prepared_arg(self) -> PreparedSideArg;

    fn with_side_arg<R>(self, f: impl FnOnce(SideArg) -> R) -> R {
        let prepared = self.into_prepared_arg();
        f(prepared.as_side_arg())
    }
}

#[doc(hidden)]
pub trait GatherType {
    const STRUCT_TYPE: SideRawPtr;
    const ELEMENT_TYPE: SideRawPtr;
}

const fn side_type_bool() -> SideType {
    SideType {
        type_: SIDE_TYPE_BOOL,
        u: SideTypePayload {
            side_bool: SideTypeBool {
                attributes: SideArray::empty(),
                bool_size: size_of::<u8>() as u16,
                len_bits: 0,
                byte_order: SIDE_TYPE_BYTE_ORDER_HOST,
            },
        },
    }
}

const fn side_type_integer(type_: u16, integer_size: u16, signedness: u8) -> SideType {
    SideType {
        type_,
        u: SideTypePayload {
            side_integer: SideTypeInteger {
                attributes: SideArray::empty(),
                integer_size,
                len_bits: 0,
                signedness,
                byte_order: SIDE_TYPE_BYTE_ORDER_HOST,
            },
        },
    }
}

const fn side_type_pointer() -> SideType {
    side_type_integer(SIDE_TYPE_POINTER, size_of::<usize>() as u16, 0)
}

const fn side_type_string_utf8() -> SideType {
    SideType {
        type_: SIDE_TYPE_STRING_UTF8,
        u: SideTypePayload {
            side_string: SideTypeString {
                attributes: SideArray::empty(),
                unit_size: size_of::<u8>() as u8,
                byte_order: SIDE_TYPE_BYTE_ORDER_HOST,
            },
        },
    }
}

pub const fn side_type_gather_bool(offset: u64) -> SideType {
    SideType {
        type_: SIDE_TYPE_GATHER_BOOL,
        u: SideTypePayload {
            side_gather: SideTypeGather {
                u: SideTypeGatherPayload {
                    side_bool: SideTypeGatherBool {
                        offset,
                        offset_bits: 0,
                        access_mode: SIDE_TYPE_GATHER_ACCESS_DIRECT,
                        type_: SideTypeBool {
                            attributes: SideArray::empty(),
                            bool_size: size_of::<u8>() as u16,
                            len_bits: 0,
                            byte_order: SIDE_TYPE_BYTE_ORDER_HOST,
                        },
                    },
                },
            },
        },
    }
}

pub const fn side_type_gather_integer(offset: u64, integer_size: u16, signedness: u8) -> SideType {
    SideType {
        type_: SIDE_TYPE_GATHER_INTEGER,
        u: SideTypePayload {
            side_gather: SideTypeGather {
                u: SideTypeGatherPayload {
                    side_integer: SideTypeGatherInteger {
                        offset,
                        offset_bits: 0,
                        access_mode: SIDE_TYPE_GATHER_ACCESS_DIRECT,
                        type_: SideTypeInteger {
                            attributes: SideArray::empty(),
                            integer_size,
                            len_bits: 0,
                            signedness,
                            byte_order: SIDE_TYPE_BYTE_ORDER_HOST,
                        },
                    },
                },
            },
        },
    }
}

pub const fn side_type_gather_pointer(offset: u64) -> SideType {
    SideType {
        type_: SIDE_TYPE_GATHER_POINTER,
        u: SideTypePayload {
            side_gather: SideTypeGather {
                u: SideTypeGatherPayload {
                    side_integer: SideTypeGatherInteger {
                        offset,
                        offset_bits: 0,
                        access_mode: SIDE_TYPE_GATHER_ACCESS_DIRECT,
                        type_: SideTypeInteger {
                            attributes: SideArray::empty(),
                            integer_size: size_of::<usize>() as u16,
                            len_bits: 0,
                            signedness: 0,
                            byte_order: SIDE_TYPE_BYTE_ORDER_HOST,
                        },
                    },
                },
            },
        },
    }
}

pub const fn side_type_gather_struct(
    type_: SideRawPtr,
    offset: u64,
    size: u32,
    access_mode: u8,
) -> SideType {
    SideType {
        type_: SIDE_TYPE_GATHER_STRUCT,
        u: SideTypePayload {
            side_gather: SideTypeGather {
                u: SideTypeGatherPayload {
                    side_struct: SideTypeGatherStruct {
                        type_,
                        offset,
                        access_mode,
                        size,
                    },
                },
            },
        },
    }
}

pub const fn side_type_gather_array(elem_type: SideRawPtr, length: u32, offset: u64) -> SideType {
    SideType {
        type_: SIDE_TYPE_GATHER_ARRAY,
        u: SideTypePayload {
            side_gather: SideTypeGather {
                u: SideTypeGatherPayload {
                    side_array: SideTypeGatherArray {
                        offset,
                        access_mode: SIDE_TYPE_GATHER_ACCESS_DIRECT,
                        type_: SideTypeArray {
                            elem_type,
                            length,
                            attributes: SideArray::empty(),
                        },
                    },
                },
            },
        },
    }
}

pub const fn side_type_gather_vla(
    elem_type: SideRawPtr,
    offset: u64,
    length_type: SideRawPtr,
) -> SideType {
    SideType {
        type_: SIDE_TYPE_GATHER_VLA,
        u: SideTypePayload {
            side_gather: SideTypeGather {
                u: SideTypeGatherPayload {
                    side_vla: SideTypeGatherVla {
                        offset,
                        access_mode: SIDE_TYPE_GATHER_ACCESS_POINTER,
                        type_: SideTypeVla {
                            elem_type,
                            length_type,
                            attributes: SideArray::empty(),
                        },
                    },
                },
            },
        },
    }
}

impl FieldType for bool {
    const FIELD_TYPE: SideType = side_type_bool();

    fn into_prepared_arg(self) -> PreparedSideArg {
        PreparedSideArg::new(SideArg::bool(self))
    }
}

impl FieldType for u8 {
    const FIELD_TYPE: SideType = side_type_integer(SIDE_TYPE_U8, size_of::<u8>() as u16, 0);

    fn into_prepared_arg(self) -> PreparedSideArg {
        PreparedSideArg::new(SideArg::u8(self))
    }
}

impl FieldType for u16 {
    const FIELD_TYPE: SideType = side_type_integer(SIDE_TYPE_U16, size_of::<u16>() as u16, 0);

    fn into_prepared_arg(self) -> PreparedSideArg {
        PreparedSideArg::new(SideArg::u16(self))
    }
}

impl FieldType for u32 {
    const FIELD_TYPE: SideType = side_type_integer(SIDE_TYPE_U32, size_of::<u32>() as u16, 0);

    fn into_prepared_arg(self) -> PreparedSideArg {
        PreparedSideArg::new(SideArg::u32(self))
    }
}

impl FieldType for u64 {
    const FIELD_TYPE: SideType = side_type_integer(SIDE_TYPE_U64, size_of::<u64>() as u16, 0);

    fn into_prepared_arg(self) -> PreparedSideArg {
        PreparedSideArg::new(SideArg::u64(self))
    }
}

impl FieldType for i8 {
    const FIELD_TYPE: SideType = side_type_integer(SIDE_TYPE_S8, size_of::<i8>() as u16, 1);

    fn into_prepared_arg(self) -> PreparedSideArg {
        PreparedSideArg::new(SideArg::s8(self))
    }
}

impl FieldType for i16 {
    const FIELD_TYPE: SideType = side_type_integer(SIDE_TYPE_S16, size_of::<i16>() as u16, 1);

    fn into_prepared_arg(self) -> PreparedSideArg {
        PreparedSideArg::new(SideArg::s16(self))
    }
}

impl FieldType for i32 {
    const FIELD_TYPE: SideType = side_type_integer(SIDE_TYPE_S32, size_of::<i32>() as u16, 1);

    fn into_prepared_arg(self) -> PreparedSideArg {
        PreparedSideArg::new(SideArg::s32(self))
    }
}

impl FieldType for i64 {
    const FIELD_TYPE: SideType = side_type_integer(SIDE_TYPE_S64, size_of::<i64>() as u16, 1);

    fn into_prepared_arg(self) -> PreparedSideArg {
        PreparedSideArg::new(SideArg::s64(self))
    }
}

impl<T> FieldType for *const T {
    const FIELD_TYPE: SideType = side_type_pointer();

    fn into_prepared_arg(self) -> PreparedSideArg {
        PreparedSideArg::new(SideArg::pointer(self))
    }
}

impl<T> FieldType for *mut T {
    const FIELD_TYPE: SideType = side_type_pointer();

    fn into_prepared_arg(self) -> PreparedSideArg {
        PreparedSideArg::new(SideArg::pointer(self.cast_const()))
    }
}

impl<'a> FieldType for &'a CStr {
    const FIELD_TYPE: SideType = side_type_string_utf8();

    fn into_prepared_arg(self) -> PreparedSideArg {
        PreparedSideArg::string(self)
    }
}

impl<'a> FieldType for &'a str {
    const FIELD_TYPE: SideType = side_type_string_utf8();

    fn into_prepared_arg(self) -> PreparedSideArg {
        PreparedSideArg::owned_string(
            CString::new(self).expect("string fields must not contain interior NUL bytes"),
        )
    }
}

const _: [(); 16] = [(); size_of::<SideRawPtr>()];
const _: [(); 20] = [(); size_of::<SideArray>()];
const _: [(); 32] = [(); size_of::<SideBoolValue>()];
const _: [(); 32] = [(); size_of::<SideIntegerValue>()];
const _: [(); 32] = [(); size_of::<SideArgStatic>()];
const _: [(); 60] = [(); size_of::<SideArgPayload>()];
const _: [(); 64] = [(); size_of::<SideArg>()];
const _: [(); 20] = [(); size_of::<SideArgVec>()];
const _: [(); 20] = [(); size_of::<SideTypeNull>()];
const _: [(); 25] = [(); size_of::<SideTypeBool>()];
const _: [(); 20] = [(); size_of::<SideTypeByte>()];
const _: [(); 22] = [(); size_of::<SideTypeString>()];
const _: [(); 26] = [(); size_of::<SideTypeInteger>()];
const _: [(); 40] = [(); size_of::<SideTypeStruct>()];
const _: [(); 40] = [(); size_of::<SideTypeArray>()];
const _: [(); 52] = [(); size_of::<SideTypeVla>()];
const _: [(); 36] = [(); size_of::<SideTypeGatherBool>()];
const _: [(); 37] = [(); size_of::<SideTypeGatherInteger>()];
const _: [(); 29] = [(); size_of::<SideTypeGatherStruct>()];
const _: [(); 49] = [(); size_of::<SideTypeGatherArray>()];
const _: [(); 61] = [(); size_of::<SideTypeGatherVla>()];
const _: [(); 61] = [(); size_of::<SideTypeGather>()];
const _: [(); 64] = [(); size_of::<SideType>()];
const _: [(); 80] = [(); size_of::<SideEventField>()];
const _: [(); 8 + 3 * size_of::<*const c_void>()] = [(); size_of::<SideEventState0>()];
const _: [(); 112] = [(); size_of::<SideEventDescription>()];

#[macro_export]
macro_rules! __emit_event_call_macro {
    ($d:tt $name:ident, $module:ident, $first:ident $(, $field:ident)*) => {
        #[allow(unused_macros)]
        macro_rules! $name {
            ($d first_value:expr, $d($d field:ident : $d value:expr),* $d(,)?) => {{
                if unsafe { $module::enabled() } {
                    let arguments = $module::__EventArguments {
                        $first: $d first_value,
                        $d($d field: $d value),*
                    };
                    $module::function(arguments.$first $(, arguments.$field)*);
                }
            }};
            ($d($d value:expr),* $d(,)?) => {{
                if unsafe { $module::enabled() } {
                    $module::function($d($d value),*);
                }
            }};
        }
    };
}

/// Declare a `#[repr(C)]` Rust struct as a libside gather struct.
///
/// The generated event field reads the listed primitive members directly from
/// the pointer passed at the call site; the struct itself is not copied.
#[macro_export]
macro_rules! define_gather_struct {
    (
        $name:ident for $struct:ident,
        fields: (
            $( $field:ident : $ty:tt ),* $(,)?
        ) $(,)?
    ) => {
        mod $name {
            use super::*;

            #[allow(dead_code)]
            fn assert_field_types(value: &$struct) {
                $(
                    let _: &$ty = &value.$field;
                )*
            }

            static GATHER_FIELDS: [$crate::side::SideEventField;
                define_gather_struct!(@count $( $field ),*)] = [
                $(
                    $crate::side::SideEventField {
                        field_name: $crate::side::SideRawPtr::from_const(
                            ::core::concat!(::core::stringify!($field), "\0").as_ptr(),
                        ),
                        side_type: define_gather_struct!(
                            @field_type $ty,
                            ::core::mem::offset_of!($struct, $field) as u64
                        ),
                    },
                )*
            ];

            static GATHER_TYPE: $crate::side::SideTypeStruct =
                $crate::side::SideTypeStruct {
                    fields: $crate::side::SideArray::new(
                        GATHER_FIELDS.as_ptr().cast(),
                        GATHER_FIELDS.len() as u32,
                    ),
                    attributes: $crate::side::SideArray::empty(),
                };

            impl<'a> $crate::side::FieldType for &'a $struct {
                const FIELD_TYPE: $crate::side::SideType = $crate::side::SideType {
                    type_: $crate::side::SIDE_TYPE_GATHER_STRUCT,
                    u: $crate::side::SideTypePayload {
                        side_gather: $crate::side::SideTypeGather {
                            u: $crate::side::SideTypeGatherPayload {
                                side_struct: $crate::side::SideTypeGatherStruct {
                                    type_: $crate::side::SideRawPtr::from_const(
                                        ::core::ptr::addr_of!(GATHER_TYPE),
                                    ),
                                    offset: 0,
                                    access_mode: $crate::side::SIDE_TYPE_GATHER_ACCESS_DIRECT,
                                    size: ::core::mem::size_of::<$struct>() as u32,
                                },
                            },
                        },
                    },
                };

                fn into_prepared_arg(self) -> $crate::side::PreparedSideArg {
                    $crate::side::PreparedSideArg::new(
                        $crate::side::SideArg::gather_struct(self as *const $struct),
                    )
                }
            }
        }
    };

    (@field_type bool, $offset:expr) => {
        $crate::side::side_type_gather_bool($offset)
    };
    (@field_type u8, $offset:expr) => {
        $crate::side::side_type_gather_integer($offset, 1, 0)
    };
    (@field_type u16, $offset:expr) => {
        $crate::side::side_type_gather_integer($offset, 2, 0)
    };
    (@field_type u32, $offset:expr) => {
        $crate::side::side_type_gather_integer($offset, 4, 0)
    };
    (@field_type u64, $offset:expr) => {
        $crate::side::side_type_gather_integer($offset, 8, 0)
    };
    (@field_type i8, $offset:expr) => {
        $crate::side::side_type_gather_integer($offset, 1, 1)
    };
    (@field_type i16, $offset:expr) => {
        $crate::side::side_type_gather_integer($offset, 2, 1)
    };
    (@field_type i32, $offset:expr) => {
        $crate::side::side_type_gather_integer($offset, 4, 1)
    };
    (@field_type i64, $offset:expr) => {
        $crate::side::side_type_gather_integer($offset, 8, 1)
    };
    (@count) => { 0usize };
    (@count $head:ident $(, $tail:ident)*) => {
        1usize + define_gather_struct!(@count $( $tail ),*)
    };
}

#[macro_export]
macro_rules! define_event {
    (
        $name:ident !,
        provider: $provider:literal,
        event: $event:literal,
        level: $level:expr,
        fields: (
            $( $arg:ident : $ty:ty ),* $(,)?
        ) $(,)?
    ) => {
        $crate::define_event!(
            $name,
            provider: $provider,
            event: $event,
            level: $level,
            fields: (
                $( $arg : $ty ),*
            ),
        );
    };

    (
        $name:ident,
        provider: $provider:literal,
        event: $event:literal,
        level: $level:expr,
        fields: (
            $( $arg:ident : $ty:ty ),* $(,)?
        ) $(,)?
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

            #[used]
            #[cfg_attr(any(target_os = "linux", target_os = "android"), link_section = "side_event_description")]
            pub static EVENT_DESC: $crate::side::SideEventDescription =
                $crate::side::SideEventDescription {
                    struct_size: ::core::mem::size_of::<$crate::side::SideEventDescription>() as u32,
                    version: $crate::side::SIDE_EVENT_DESCRIPTION_ABI_VERSION,
                    state: $crate::side::SideRawPtr::from_const(
                        ::core::ptr::addr_of_mut!(EVENT_STATE).cast::<$crate::side::SideEventState>(),
                    ),
                    provider_name: $crate::side::SideRawPtr::from_const(
                        ::core::concat!($provider, "\0").as_ptr(),
                    ),
                    event_name: $crate::side::SideRawPtr::from_const(
                        ::core::concat!($event, "\0").as_ptr(),
                    ),
                    fields: $crate::side::SideArray::new(
                        EVENT_FIELDS.as_ptr().cast(),
                        EVENT_FIELDS.len() as u32,
                    ),
                    attributes: $crate::side::SideArray::empty(),
                    flags: 0,
                    nr_side_type_label: $crate::side::SIDE_NR_TYPE_LABEL,
                    nr_side_attr_type: $crate::side::SIDE_NR_ATTR_TYPE,
                    loglevel: $level,
                };

            #[used]
            #[cfg_attr(any(target_os = "linux", target_os = "android"), link_section = "side_event_state")]
            pub static mut EVENT_STATE: $crate::side::SideEventState0 =
                $crate::side::SideEventState0 {
                    parent: $crate::side::SideEventState {
                        version: $crate::side::SIDE_EVENT_STATE_ABI_VERSION,
                    },
                    nr_callbacks: 0,
                    enabled: 0,
                    callbacks: ::core::ptr::addr_of!($crate::side::side_empty_callback).cast(),
                    desc: ::core::ptr::addr_of!(EVENT_DESC),
                };

            #[used]
            #[cfg_attr(any(target_os = "linux", target_os = "android"), link_section = "side_event_description_ptr")]
            static mut EVENT_DESC_PTR: *mut $crate::side::SideEventDescription =
                ::core::ptr::addr_of!(EVENT_DESC).cast_mut();

            static mut EVENT_REGISTER_HANDLE: *mut $crate::side::SideEventsRegisterHandle =
                ::core::ptr::null_mut();

            unsafe extern "C" fn register_event() {
                unsafe {
                    EVENT_REGISTER_HANDLE = $crate::side::side_events_register(
                        ::core::ptr::addr_of_mut!(EVENT_DESC_PTR),
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

            static EVENT_FIELDS: [$crate::side::SideEventField; define_event!(@count $( $arg ),*)] = [
                $(
                    $crate::side::SideEventField {
                        field_name: $crate::side::SideRawPtr::from_const(
                            ::core::concat!(::core::stringify!($arg), "\0").as_ptr(),
                        ),
                        side_type: <$ty as $crate::side::FieldType>::FIELD_TYPE,
                    }
                ),*
            ];

            pub(crate) unsafe fn enabled() -> bool {
                let enabled_ptr = ::core::ptr::addr_of!((*::core::ptr::addr_of_mut!(EVENT_STATE)).enabled);
                unsafe { ::core::ptr::read_volatile(enabled_ptr) != 0 }
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

    (@count) => { 0usize };
    (@count $head:ident $(, $tail:ident)*) => { 1usize + define_event!(@count $( $tail ),*) };

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
