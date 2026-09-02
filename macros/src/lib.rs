use proc_macro::{Delimiter, Group, TokenStream, TokenTree};

#[proc_macro_derive(SideGather)]
pub fn derive_side_gather(input: TokenStream) -> TokenStream {
    match derive_side_gather_impl(input) {
        Ok(output) => output.parse().expect("generated valid Rust"),
        Err(message) => format!("compile_error!({message:?});")
            .parse()
            .expect("generated valid compile error"),
    }
}

fn derive_side_gather_impl(input: TokenStream) -> Result<String, String> {
    let mut tokens = input.into_iter();
    let mut struct_name = None;
    let mut fields = None;

    while let Some(token) = tokens.next() {
        match token {
            TokenTree::Ident(ident) if ident.to_string() == "struct" => {
                let Some(TokenTree::Ident(name)) = tokens.next() else {
                    return Err("SideGather requires a named struct".into());
                };
                struct_name = Some(name.to_string());
            }
            TokenTree::Group(group) if group.delimiter() == Delimiter::Brace => {
                fields = Some(parse_fields(group)?);
                break;
            }
            _ => {}
        }
    }

    let struct_name = struct_name.ok_or("SideGather can only be derived for structs")?;
    let fields = fields.ok_or("SideGather requires a struct with named fields")?;
    let module_name = format!("__side_gather_for_{struct_name}");
    let view_name = format!("__side_gather_view_for_{struct_name}");
    let has_vec = fields.iter().any(|(_, type_)| vec_element_type(type_).is_some());

    let mut metadata = String::new();
    let mut vla_length_types = String::new();
    let mut assertions = String::new();
    let mut view_fields = String::new();
    let mut view_values = String::new();
    for (field_name, field_type) in &fields {
        let metadata_type = if let Some(element_type) = vec_element_type(field_type) {
            format!("SideVla<{element_type}>")
        } else {
            field_type.clone()
        };
        let metadata_struct = if has_vec { &view_name } else { &struct_name };
        let offset = format!("::core::mem::offset_of!({metadata_struct}, {field_name}) as u64");
        let (gather_type, vla_length_type) = gather_type(metadata_struct, field_name, &metadata_type, &offset)?;
        metadata.push_str(&format!(
            "::libside::side::SideEventField {{ field_name: ::libside::side::SideRawPtr::from_const(::core::concat!(::core::stringify!({field_name}), \"\\0\").as_ptr()), side_type: {gather_type}, }},"
        ));
        vla_length_types.push_str(&vla_length_type.unwrap_or_default());
        let assertion_type = field_type.replace("& ' ", "&'");
        assertions.push_str(&format!("let _: &{assertion_type} = &value.{field_name};"));
        if has_vec {
            if let Some(element_type) = vec_element_type(field_type) {
                view_fields.push_str(&format!("{field_name}: ::libside::side::SideVla<{element_type}>,"));
                view_values.push_str(&format!("{field_name}: ::libside::side::SideVla::from_slice(&self.{field_name}),"));
            } else if is_primitive(field_type) {
                view_fields.push_str(&format!("{field_name}: {field_type},"));
                view_values.push_str(&format!("{field_name}: self.{field_name},"));
            } else {
                return Err("SIDE structs containing Vec members currently support primitive and Vec fields".into());
            }
        }
    }

    let view_definition = if has_vec {
        format!("#[repr(C)] struct {view_name} {{ {view_fields} }}")
    } else {
        String::new()
    };
    let gather_struct = if has_vec { &view_name } else { &struct_name };
    let prepared_arg = if has_vec {
        format!("::libside::side::PreparedSideArg::owned_gather({view_name} {{ {view_values} }})")
    } else {
        format!("::libside::side::PreparedSideArg::new(::libside::side::SideArg::gather_struct(self as *const {struct_name}))")
    };
    let with_side_arg = if has_vec {
        format!(
            "fn with_side_arg<R>(self, f: impl FnOnce(::libside::side::SideArg) -> R) -> R {{ let view = {view_name} {{ {view_values} }}; f(::libside::side::SideArg::gather_struct(::core::ptr::addr_of!(view))) }}"
        )
    } else {
        String::new()
    };

    let output = format!(
        "#[doc(hidden)] mod {module_name} {{
            use super::*;
            #[allow(dead_code)] fn assert_field_types(value: &{struct_name}) {{ {assertions} }}
            {view_definition}
            {vla_length_types}
            static FIELDS: [::libside::side::SideEventField; {field_count}] = [{metadata}];
            static TYPE: ::libside::side::SideTypeStruct = ::libside::side::SideTypeStruct {{
                fields: ::libside::side::SideArray::new(FIELDS.as_ptr().cast(), FIELDS.len() as u32),
                attributes: ::libside::side::SideArray::empty(),
            }};
            static GATHER_ELEMENT_TYPE: ::libside::side::SideType = ::libside::side::SideType {{
                type_: ::libside::side::SIDE_TYPE_GATHER_STRUCT,
                u: ::libside::side::SideTypePayload {{
                    side_gather: ::libside::side::SideTypeGather {{
                        u: ::libside::side::SideTypeGatherPayload {{
                            side_struct: ::libside::side::SideTypeGatherStruct {{
                                type_: ::libside::side::SideRawPtr::from_const(::core::ptr::addr_of!(TYPE)),
                                offset: 0,
                                access_mode: ::libside::side::SIDE_TYPE_GATHER_ACCESS_DIRECT,
                                size: ::core::mem::size_of::<{gather_struct}>() as u32,
                            }},
                        }},
                    }},
                }},
            }};
            impl<'a> ::libside::side::FieldType for &'a {struct_name} {{
                const FIELD_TYPE: ::libside::side::SideType = ::libside::side::SideType {{
                    type_: ::libside::side::SIDE_TYPE_GATHER_STRUCT,
                    u: ::libside::side::SideTypePayload {{
                        side_gather: ::libside::side::SideTypeGather {{
                            u: ::libside::side::SideTypeGatherPayload {{
                                side_struct: ::libside::side::SideTypeGatherStruct {{
                                    type_: ::libside::side::SideRawPtr::from_const(::core::ptr::addr_of!(TYPE)),
                                    offset: 0,
                                    access_mode: ::libside::side::SIDE_TYPE_GATHER_ACCESS_DIRECT,
                                    size: ::core::mem::size_of::<{gather_struct}>() as u32,
                                }},
                            }},
                        }},
                    }},
                }};
                fn into_prepared_arg(self) -> ::libside::side::PreparedSideArg {{
                    {prepared_arg}
                }}
                {with_side_arg}
            }}
            impl ::libside::side::GatherType for {struct_name} {{
                const STRUCT_TYPE: ::libside::side::SideRawPtr =
                    ::libside::side::SideRawPtr::from_const(::core::ptr::addr_of!(TYPE));
                const ELEMENT_TYPE: ::libside::side::SideRawPtr =
                    ::libside::side::SideRawPtr::from_const(::core::ptr::addr_of!(GATHER_ELEMENT_TYPE));
            }}
        }}",
        field_count = fields.len(),
    );
    Ok(output)
}

fn parse_fields(group: Group) -> Result<Vec<(String, String)>, String> {
    let mut fields = Vec::new();
    for field in split_on_comma(group.stream()) {
        if field.is_empty() {
            continue;
        }
        let colon = field
            .iter()
            .position(|token| matches!(token, TokenTree::Punct(punct) if punct.as_char() == ':'))
            .ok_or("SideGather requires named fields")?;
        let name = field[..colon]
            .iter()
            .rev()
            .find_map(|token| match token {
                TokenTree::Ident(ident) => Some(ident.to_string()),
                _ => None,
            })
            .ok_or("SideGather requires named fields")?;
        let type_ = field[colon + 1..]
            .iter()
            .map(TokenTree::to_string)
            .collect::<Vec<_>>()
            .join(" ");
        fields.push((name, type_));
    }
    Ok(fields)
}

fn split_on_comma(stream: TokenStream) -> Vec<Vec<TokenTree>> {
    let mut fields = vec![Vec::new()];
    for token in stream {
        if matches!(&token, TokenTree::Punct(punct) if punct.as_char() == ',') {
            fields.push(Vec::new());
        } else {
            fields.last_mut().unwrap().push(token);
        }
    }
    fields
}

fn gather_type(
    struct_name: &str,
    field_name: &str,
    type_: &str,
    offset: &str,
) -> Result<(String, Option<String>), String> {
    let compact_type = type_.replace(' ', "");
    if let Some(element_type) = compact_type
        .strip_prefix("SideVla<")
        .and_then(|type_| type_.strip_suffix('>'))
    {
        let length_name = format!("VLA_LENGTH_TYPE_{field_name}");
        let length_offset = format!(
            "::core::mem::offset_of!({struct_name}, {field_name}) as u64 + ::core::mem::offset_of!(::libside::side::SideVla<{element_type}>, len) as u64"
        );
        let length_type = format!(
            "static {length_name}: ::libside::side::SideType = ::libside::side::side_type_gather_integer({length_offset}, ::core::mem::size_of::<usize>() as u16, 0);"
        );
        return Ok((
            format!(
                "::libside::side::side_type_gather_vla(<{element_type} as ::libside::side::GatherType>::ELEMENT_TYPE, {offset}, ::libside::side::SideRawPtr::from_const(::core::ptr::addr_of!({length_name})))"
            ),
            Some(length_type),
        ));
    }

    if let Some(array) = type_.strip_prefix('[').and_then(|type_| type_.strip_suffix(']')) {
        let (element_type, length) = array
            .split_once(';')
            .ok_or("SIDE arrays require an element type and length")?;
        return Ok((
            format!(
                "::libside::side::side_type_gather_array(<{element_type} as ::libside::side::GatherType>::ELEMENT_TYPE, {length}, {offset})"
            ),
            None,
        ));
    }

    let gather_type = match type_ {
        "bool" => format!("::libside::side::side_type_gather_bool({offset})"),
        "u8" => format!("::libside::side::side_type_gather_integer({offset}, 1, 0)"),
        "u16" => format!("::libside::side::side_type_gather_integer({offset}, 2, 0)"),
        "u32" => format!("::libside::side::side_type_gather_integer({offset}, 4, 0)"),
        "u64" => format!("::libside::side::side_type_gather_integer({offset}, 8, 0)"),
        "i8" => format!("::libside::side::side_type_gather_integer({offset}, 1, 1)"),
        "i16" => format!("::libside::side::side_type_gather_integer({offset}, 2, 1)"),
        "i32" => format!("::libside::side::side_type_gather_integer({offset}, 4, 1)"),
        "i64" => format!("::libside::side::side_type_gather_integer({offset}, 8, 1)"),
        _ => gather_struct_type(type_, offset)?,
    };
    Ok((gather_type, None))
}

fn vec_element_type(type_: &str) -> Option<String> {
    let type_ = type_.replace(' ', "");
    type_
        .strip_prefix("Vec<")
        .and_then(|type_| type_.strip_suffix('>'))
        .map(str::to_owned)
}

fn is_primitive(type_: &str) -> bool {
    matches!(type_, "bool" | "u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64")
}

fn gather_struct_type(type_: &str, offset: &str) -> Result<String, String> {
    let type_ = type_.replace(' ', "");
    let (type_, access_mode) = if let Some(type_) = type_.strip_prefix('&') {
        (
            type_.strip_prefix("'static").unwrap_or(type_),
            "::libside::side::SIDE_TYPE_GATHER_ACCESS_POINTER",
        )
    } else {
        (
            type_.as_str(),
            "::libside::side::SIDE_TYPE_GATHER_ACCESS_DIRECT",
        )
    };

    if type_.is_empty() {
        return Err("SideGather requires a concrete referenced struct type".into());
    }

    Ok(format!(
        "::libside::side::side_type_gather_struct(<{type_} as ::libside::side::GatherType>::STRUCT_TYPE, {offset}, ::core::mem::size_of::<{type_}>() as u32, {access_mode})"
    ))
}
