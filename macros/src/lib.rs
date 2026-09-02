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
    let has_vec = fields
        .iter()
        .any(|(_, type_)| vec_element_type(type_).is_some());

    /*
     * A Vec is three words in an order Rust does not promise, so a
     * structure holding one is described through a view whose vectors
     * are laid out the way libside reads them. Everything a member
     * offset is measured against is then that view.
     */
    let layout_struct = if has_vec { &view_name } else { &struct_name };

    let mut layouts = String::new();
    let mut assertions = String::new();
    let mut view_fields = String::new();
    let mut view_values = String::new();
    for (field_name, field_type) in &fields {
        let member_type = if let Some(element_type) = vec_element_type(field_type) {
            format!("::libside::side::SideVla<{element_type}>")
        } else {
            field_type.clone()
        };
        let offset = format!("::core::mem::offset_of!({layout_struct}, {field_name}) as u64");
        let layout = member_layout(layout_struct, field_name, &member_type, &offset)?;
        layouts.push_str(&format!(
            "::libside::side::Field {{ name: ::core::stringify!({field_name}), layout: {layout} }},"
        ));

        let assertion_type = field_type.replace("& ' ", "&'");
        assertions.push_str(&format!("let _: &{assertion_type} = &value.{field_name};"));

        if has_vec {
            if let Some(element_type) = vec_element_type(field_type) {
                view_fields.push_str(&format!(
                    "{field_name}: ::libside::side::SideVla<{element_type}>,"
                ));
                view_values.push_str(&format!(
                    "{field_name}: ::libside::side::SideVla::from_slice(&self.{field_name}),"
                ));
            } else if is_primitive(field_type) {
                view_fields.push_str(&format!("{field_name}: {field_type},"));
                view_values.push_str(&format!("{field_name}: self.{field_name},"));
            } else {
                return Err(
                    "SIDE structs containing Vec members currently support primitive and Vec fields"
                        .into(),
                );
            }
        }
    }

    let view_definition = if has_vec {
        format!("#[repr(C)] struct {view_name} {{ {view_fields} }}")
    } else {
        String::new()
    };
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
            impl ::libside::side::GatherType for {struct_name} {{
                const FIELDS: &'static [::libside::side::Field] = &[{layouts}];
                const SIZE: u32 = ::core::mem::size_of::<{layout_struct}>() as u32;
            }}
            impl<'a> ::libside::side::FieldType for &'a {struct_name} {{
                const LAYOUT: ::libside::side::Layout = ::libside::side::Layout::GatherStruct {{
                    offset: 0,
                    size: <{struct_name} as ::libside::side::GatherType>::SIZE,
                    access: ::libside::side::SIDE_TYPE_GATHER_ACCESS_DIRECT,
                    fields: <{struct_name} as ::libside::side::GatherType>::FIELDS,
                }};
                fn into_prepared_arg(self) -> ::libside::side::PreparedSideArg {{
                    {prepared_arg}
                }}
                {with_side_arg}
            }}
        }}"
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

/// The shape of one member, as `libside::side::Layout` spells it.
fn member_layout(
    struct_name: &str,
    field_name: &str,
    type_: &str,
    offset: &str,
) -> Result<String, String> {
    let compact_type = type_.replace(' ', "");

    if let Some(element_type) = compact_type
        .strip_prefix("::libside::side::SideVla<")
        .and_then(|type_| type_.strip_suffix('>'))
    {
        /*
         * The length is a member of the vector, so its offset is that
         * of the vector plus where the length sits within it.
         */
        let len_offset = format!(
            "::core::mem::offset_of!({struct_name}, {field_name}) as u64 + ::core::mem::offset_of!(::libside::side::SideVla<{element_type}>, len) as u64"
        );
        return Ok(format!(
            "::libside::side::Layout::GatherVla {{ offset: {offset}, len_offset: {len_offset}, elem: <{element_type} as ::libside::side::GatherType>::ELEMENT }}"
        ));
    }

    if let Some(array) = type_
        .strip_prefix('[')
        .and_then(|type_| type_.strip_suffix(']'))
    {
        let (element_type, length) = array
            .split_once(';')
            .ok_or("SIDE arrays require an element type and length")?;
        return Ok(format!(
            "::libside::side::Layout::GatherArray {{ offset: {offset}, length: ({length}) as u32, elem: <{element_type} as ::libside::side::GatherType>::ELEMENT }}"
        ));
    }

    let integer = |size: u16, signed: bool| {
        format!(
            "::libside::side::Layout::GatherInteger {{ offset: {offset}, size: {size}, signed: {signed} }}"
        )
    };

    Ok(match type_ {
        "bool" => format!("::libside::side::Layout::GatherBool {{ offset: {offset}, size: 1 }}"),
        "u8" => integer(1, false),
        "u16" => integer(2, false),
        "u32" => integer(4, false),
        "u64" => integer(8, false),
        "i8" => integer(1, true),
        "i16" => integer(2, true),
        "i32" => integer(4, true),
        "i64" => integer(8, true),
        _ => nested_struct_layout(type_, offset)?,
    })
}

fn vec_element_type(type_: &str) -> Option<String> {
    let type_ = type_.replace(' ', "");
    type_
        .strip_prefix("Vec<")
        .and_then(|type_| type_.strip_suffix('>'))
        .map(str::to_owned)
}

fn is_primitive(type_: &str) -> bool {
    matches!(
        type_,
        "bool" | "u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64"
    )
}

fn nested_struct_layout(type_: &str, offset: &str) -> Result<String, String> {
    let type_ = type_.replace(' ', "");
    /* A reference is read by dereferencing it, a value in place. */
    let (type_, access) = if let Some(type_) = type_.strip_prefix('&') {
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
        "::libside::side::Layout::GatherStruct {{ offset: {offset}, size: <{type_} as ::libside::side::GatherType>::SIZE, access: {access}, fields: <{type_} as ::libside::side::GatherType>::FIELDS }}"
    ))
}

/// Declare a group of events which share the descriptions of their types.
///
/// A description is laid out by the const evaluator into one object, and
/// every distance within it is between two bytes of that object. Two
/// events are two objects, so a type used by both is described twice --
/// unless they are laid out together, which is what this does. It is
/// the same boundary a C translation unit gives libside, and the same
/// thing a tracepoint provider has always been.
#[proc_macro_attribute]
pub fn events(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return "compile_error!(\"side: #[events] takes no argument\");"
            .parse()
            .expect("generated valid compile error");
    }
    match events_impl(item) {
        Ok(output) => output.parse().expect("generated valid Rust"),
        Err(message) => format!("compile_error!({message:?});")
            .parse()
            .expect("generated valid compile error"),
    }
}

/// A field is either a Rust type, or a structure described elsewhere.
enum FieldKind {
    /// The Rust type, which is also the type of the argument.
    Rust(String),
    /// The path of a `define_type!`, whose description is reached by
    /// address rather than by a distance.
    Extern(String),
}

struct Event {
    name: String,
    provider: String,
    event: String,
    level: String,
    fields: Vec<(String, FieldKind)>,
}

fn events_impl(item: TokenStream) -> Result<String, String> {
    let mut prefix = Vec::new();
    let mut body = None;
    let mut name = None;
    let mut tokens = item.into_iter().peekable();

    while let Some(token) = tokens.next() {
        match &token {
            TokenTree::Ident(ident) if ident.to_string() == "mod" => {
                let Some(TokenTree::Ident(module)) = tokens.next() else {
                    return Err("side: #[events] expects a named module".into());
                };
                name = Some(module.to_string());
                let Some(TokenTree::Group(group)) = tokens.next() else {
                    return Err("side: #[events] expects a module with a body".into());
                };
                body = Some(group.stream());
                break;
            }
            _ => prefix.push(token),
        }
    }

    let name = name.ok_or("side: #[events] applies to a module")?;
    let body = body.ok_or("side: #[events] applies to a module")?;

    let mut passthrough = Vec::new();
    let mut events = Vec::new();
    let mut tokens = body.into_iter().peekable();

    while let Some(token) = tokens.next() {
        let is_event = matches!(&token, TokenTree::Ident(ident) if ident.to_string() == "define_event")
            && matches!(tokens.peek(), Some(TokenTree::Punct(punct)) if punct.as_char() == '!');
        if !is_event {
            passthrough.push(token);
            continue;
        }
        tokens.next();
        let Some(TokenTree::Group(group)) = tokens.next() else {
            return Err("side: define_event! expects its declaration in brackets".into());
        };
        events.push(parse_event(group.stream())?);
        /* The trailing semicolon of the invocation, if it has one. */
        if matches!(tokens.peek(), Some(TokenTree::Punct(punct)) if punct.as_char() == ';') {
            tokens.next();
        }
    }

    if events.is_empty() {
        return Err("side: #[events] found no define_event! in the module".into());
    }

    let passthrough = passthrough
        .into_iter()
        .collect::<TokenStream>()
        .to_string();
    let prefix = prefix.into_iter().collect::<TokenStream>().to_string();

    Ok(format!(
        "{prefix} mod {name} {{ {passthrough} {} }}",
        group_body(&events)
    ))
}

fn parse_event(stream: TokenStream) -> Result<Event, String> {
    let parts = split_on_comma(stream);
    let mut parts = parts.iter().filter(|part| !part.is_empty());

    let first = parts.next().ok_or("side: define_event! expects a name")?;
    let TokenTree::Ident(name) = &first[0] else {
        return Err("side: define_event! expects a name".into());
    };

    let mut event = Event {
        name: name.to_string(),
        provider: String::new(),
        event: String::new(),
        level: String::new(),
        fields: Vec::new(),
    };

    for part in parts {
        let colon = part
            .iter()
            .position(|token| matches!(token, TokenTree::Punct(punct) if punct.as_char() == ':'))
            .ok_or("side: define_event! expects `key: value'")?;
        let key = part[..colon]
            .iter()
            .map(TokenTree::to_string)
            .collect::<String>();
        let value = part[colon + 1..]
            .iter()
            .cloned()
            .collect::<TokenStream>()
            .to_string();
        match key.as_str() {
            "provider" => event.provider = value,
            "event" => event.event = value,
            "level" => event.level = value,
            "fields" => {
                let TokenTree::Group(group) = &part[colon + 1] else {
                    return Err("side: the fields of an event go in brackets".into());
                };
                for field in split_on_comma(group.stream()) {
                    if field.is_empty() {
                        continue;
                    }
                    let colon = field
                        .iter()
                        .position(|token| {
                            matches!(token, TokenTree::Punct(punct) if punct.as_char() == ':')
                        })
                        .ok_or("side: an event field is written `name: type'")?;
                    let TokenTree::Ident(name) = &field[colon - 1] else {
                        return Err("side: an event field is written `name: type'".into());
                    };
                    event
                        .fields
                        .push((name.to_string(), parse_field_kind(&field[colon + 1..])?));
                }
            }
            other => return Err(format!("side: unknown key `{other}' in define_event!")),
        }
    }

    if event.provider.is_empty() || event.event.is_empty() || event.level.is_empty() {
        return Err("side: an event needs a provider, an event name and a level".into());
    }
    Ok(event)
}

/// `side_extern(PATH)' names a structure described elsewhere; anything
/// else is a Rust type.
fn parse_field_kind(tokens: &[TokenTree]) -> Result<FieldKind, String> {
    if let (Some(TokenTree::Ident(ident)), Some(TokenTree::Group(group))) =
        (tokens.first(), tokens.get(1))
    {
        if ident.to_string() == "side_extern" && group.delimiter() == Delimiter::Parenthesis {
            if tokens.len() != 2 {
                return Err("side: side_extern() takes the name of a define_type!".into());
            }
            let path = group.stream().to_string();
            if path.is_empty() {
                return Err("side: side_extern() takes the name of a define_type!".into());
            }
            return Ok(FieldKind::Extern(path));
        }
    }
    Ok(FieldKind::Rust(
        tokens.iter().cloned().collect::<TokenStream>().to_string(),
    ))
}

/// The Rust type of the argument a field takes.
fn argument_type(kind: &FieldKind) -> String {
    match kind {
        FieldKind::Rust(type_) => type_.clone(),
        FieldKind::Extern(path) => format!("&{path}::Type"),
    }
}

fn group_body(events: &[Event]) -> String {
    /* Each structure described elsewhere, in order of first mention. */
    let mut targets: Vec<String> = Vec::new();
    for event in events {
        for (_, kind) in &event.fields {
            if let FieldKind::Extern(path) = kind {
                if !targets.iter().any(|known| known == path) {
                    targets.push(path.clone());
                }
            }
        }
    }

    let mut specs = String::new();
    let mut field_lists = String::new();
    let mut states = String::new();
    let mut state_ptrs = String::new();
    let mut calls = String::new();

    for (i, event) in events.iter().enumerate() {
        let fields = event
            .fields
            .iter()
            .map(|(name, kind)| {
                let layout = match kind {
                    FieldKind::Rust(type_) => {
                        format!("<{type_} as ::libside::side::FieldType>::LAYOUT")
                    }
                    FieldKind::Extern(path) => {
                        let target = targets.iter().position(|known| known == path).unwrap();
                        format!(
                            "::libside::side::Layout::ExternStruct {{ offset: 0, size: {path}::SIZE, access: ::libside::side::SIDE_TYPE_GATHER_ACCESS_DIRECT, target: {target} }}"
                        )
                    }
                };
                format!("::libside::side::Field {{ name: \"{name}\", layout: {layout} }},")
            })
            .collect::<String>();
        field_lists.push_str(&format!(
            "const FIELDS_{i}: &[::libside::side::Field] = &[{fields}];"
        ));
        specs.push_str(&format!(
            "::libside::side::EventSpec {{ provider: {}, event: {}, loglevel: {}, flags: 0, fields: FIELDS_{i} }},",
            event.provider, event.event, event.level
        ));

        states.push_str(&format!(
            r#"
            #[used]
            #[cfg_attr(any(target_os = "linux", target_os = "android"), link_section = "side_event_state")]
            pub static mut STATE_{i}: ::libside::side::SideEventState0 = ::libside::side::SideEventState0 {{
                parent: ::libside::side::SideEventState {{
                    version: ::libside::side::SIDE_EVENT_STATE_ABI_VERSION,
                }},
                nr_callbacks: 0,
                enabled: 0,
                callbacks: ::core::ptr::addr_of!(::libside::side::side_empty_callback).cast(),
                desc: unsafe {{
                    ::core::ptr::addr_of_mut!(DESC)
                        .cast::<u8>()
                        .add(::libside::side::event_offset({i}))
                        .cast::<::libside::side::SideEventDescription>()
                }},
            }};
"#
        ));
        state_ptrs.push_str(&format!(
            "::core::ptr::addr_of_mut!(STATE_{i}).cast::<::libside::side::SideEventState>(),"
        ));

        let arguments = event
            .fields
            .iter()
            .map(|(name, kind)| (name.clone(), argument_type(kind)))
            .collect::<Vec<_>>();
        let signature = arguments
            .iter()
            .map(|(name, type_)| format!("{name}: {type_},"))
            .collect::<String>();
        let name = &event.name;
        calls.push_str(&format!(
            r#"
        /// Whether a tracer is listening for this event.
        ///
        /// Only worth asking where working out the arguments costs
        /// something: the event itself asks before it reads any of them.
        #[inline(always)]
        pub fn {name}_enabled() -> bool {{
            unsafe {{
                let enabled = ::core::ptr::addr_of!(
                    (*::core::ptr::addr_of_mut!(__side::STATE_{i})).enabled);
                ::core::ptr::read_volatile(enabled) != 0
            }}
        }}

        #[inline(always)]
        pub fn {name}({signature}) {{
            if !{name}_enabled() {{
                return;
            }}
            let state = unsafe {{
                ::core::ptr::addr_of!((*::core::ptr::addr_of_mut!(__side::STATE_{i})).parent)
            }};
            {}
        }}
"#,
            with_side_args(&arguments)
        ));
    }

    let count = events.len();
    let ntargets = targets.len();
    /*
     * One address is written per reference, not per structure: two
     * fields naming the same one are two places to write it.
     */
    let ntargets_refs = events
        .iter()
        .flat_map(|event| &event.fields)
        .filter(|(_, kind)| matches!(kind, FieldKind::Extern(_)))
        .count();
    let target_table = targets
        .iter()
        .map(|path| {
            format!(
                "::libside::side::SideRawPtr::from_ptr(unsafe {{ ::core::ptr::addr_of_mut!({path}::DESC) }}.cast()),"
            )
        })
        .collect::<String>();

    format!(
        r#"
        #[doc(hidden)]
        #[allow(non_snake_case, non_upper_case_globals)]
        pub mod __side {{
            use super::*;

            {field_lists}

            pub const SPECS: &[::libside::side::EventSpec] = &[{specs}];
            pub const SIZE: usize = ::libside::side::group_size(SPECS);

            /* One address per reference to a structure described elsewhere. */
            pub const NR_PATCHES: usize = {ntargets_refs};

            const BUILT: ::libside::side::Built<SIZE, NR_PATCHES> =
                ::libside::side::build_group::<SIZE, NR_PATCHES>(SPECS);

            /*
             * Every description of the group in one object, sharing one
             * copy of each type they describe the same way.
             */
            #[repr(C, align(16))]
            pub struct GroupDesc(pub [u8; SIZE]);

            #[used]
            #[cfg_attr(any(target_os = "linux", target_os = "android"), link_section = "side_event_description")]
            pub static mut DESC: GroupDesc = GroupDesc(BUILT.bytes);

            /*
             * The structures described elsewhere, and where in the
             * descriptions their addresses go. See side::Patch.
             */
            static TARGETS: [::libside::side::SideRawPtr; {ntargets}] = [{target_table}];
            const PATCHES: [::libside::side::Patch; NR_PATCHES] = BUILT.patches;

            {states}

            #[used]
            #[cfg_attr(any(target_os = "linux", target_os = "android"), link_section = "side_event_state_ptr")]
            static mut STATE_PTRS: [*mut ::libside::side::SideEventState; {count}] = [{state_ptrs}];

            static mut HANDLE: *mut ::libside::side::SideEventsRegisterHandle =
                ::core::ptr::null_mut();

            unsafe extern "C" fn register() {{
                unsafe {{
                    /*
                     * Write the address of each structure described
                     * elsewhere, which is the work a loader does for a
                     * relocation and costs the same.
                     */
                    let base = ::core::ptr::addr_of_mut!(DESC).cast::<u8>();
                    let mut i = 0;
                    while i < NR_PATCHES {{
                        let patch = PATCHES[i];
                        base.add(patch.at)
                            .cast::<::libside::side::SideRawPtr>()
                            .write_unaligned(TARGETS[patch.target]);
                        i += 1;
                    }}

                    HANDLE = ::libside::side::side_events_register(
                        ::core::ptr::addr_of_mut!(STATE_PTRS).cast(), {count});
                }}
            }}

            unsafe extern "C" fn unregister() {{
                unsafe {{ ::libside::side::side_events_unregister(HANDLE); }}
            }}

            #[used]
            #[cfg_attr(any(target_os = "linux", target_os = "android"), link_section = ".init_array")]
            static REGISTER: unsafe extern "C" fn() = register;

            #[used]
            #[cfg_attr(any(target_os = "linux", target_os = "android"), link_section = ".fini_array")]
            static UNREGISTER: unsafe extern "C" fn() = unregister;
        }}

        {calls}
"#
    )
}

/// The nest of closures which keeps every argument alive until the call.
fn with_side_args(arguments: &[(String, String)]) -> String {
    let mut body = format!(
        "unsafe {{ ::libside::side::call(state, &[{}]); }}",
        arguments
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>()
            .join(", ")
    );
    for (name, type_) in arguments.iter().rev() {
        body = format!(
            "<{type_} as ::libside::side::FieldType>::with_side_arg({name}, |{name}| {{ {body} }})"
        );
    }
    body
}
