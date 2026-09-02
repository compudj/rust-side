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
            "::libside::side::Field {{ name: ::core::stringify!({field_name}), layout: {layout}, attributes: &[] }},"
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
///
/// A group may also dump the state a tracer missed by starting late:
///
/// ```ignore
/// #[libside::events(statedump = dump, mode = agent_thread)]
/// mod trace { ... }
/// ```
///
/// registers `dump` as the group's state dump callback, from the same
/// constructor which registers the events and after them -- which is
/// the order it has to be, since registering the callback takes a dump
/// at once. The mode says who runs the callback: `agent_thread`, a
/// thread libside spawns, or `polling`, the application, which the
/// group then gives `statedump_pending()` and
/// `run_pending_statedumps()` for.
#[proc_macro_attribute]
pub fn events(attr: TokenStream, item: TokenStream) -> TokenStream {
    let statedump = match parse_group_statedump(attr) {
        Ok(statedump) => statedump,
        Err(message) => {
            return format!("compile_error!({message:?});")
                .parse()
                .expect("generated valid compile error")
        }
    };
    match events_impl(item, statedump.as_ref()) {
        Ok(output) => output.parse().expect("generated valid Rust"),
        Err(message) => format!("compile_error!({message:?});")
            .parse()
            .expect("generated valid compile error"),
    }
}

/// The state dump of a group: whose callback, and who runs it.
struct GroupStatedump {
    /// The path of the callback, which takes a `StatedumpKey`.
    callback: String,
    /// The `side::StatedumpMode` variant, spelled as libside spells it.
    mode: String,
}

/// Read `statedump = <path>, mode = polling | agent_thread`.
///
/// The mode is not defaulted. libside makes a C caller say it because
/// the two are different bargains -- one spawns a thread, the other
/// leaves the application owing a call -- and neither is a quiet
/// default to hand somebody who wrote only `statedump = dump`.
fn parse_group_statedump(attr: TokenStream) -> Result<Option<GroupStatedump>, String> {
    let parts = split_on_comma(attr);
    let mut callback = None;
    let mut mode = None;

    for part in parts.iter().filter(|part| !part.is_empty()) {
        let equals = part
            .iter()
            .position(|token| matches!(token, TokenTree::Punct(punct) if punct.as_char() == '='))
            .ok_or("side: #[events] takes `statedump = <callback>, mode = <mode>'")?;
        let key = part[..equals]
            .iter()
            .map(TokenTree::to_string)
            .collect::<String>();
        let value = part[equals + 1..]
            .iter()
            .cloned()
            .collect::<TokenStream>()
            .to_string();
        match key.as_str() {
            "statedump" => callback = Some(value),
            "mode" => {
                mode = Some(match value.as_str() {
                    "polling" => "Polling",
                    "agent_thread" => "AgentThread",
                    _ => {
                        return Err(format!(
                            "side: `{value}' is not a state dump mode; \
                             it is `polling' or `agent_thread'"
                        ))
                    }
                });
            }
            other => {
                return Err(format!(
                    "side: #[events] knows `statedump' and `mode', not `{other}'"
                ))
            }
        }
    }

    match (callback, mode) {
        (None, None) => Ok(None),
        (Some(callback), Some(mode)) => Ok(Some(GroupStatedump {
            callback,
            mode: mode.to_string(),
        })),
        (Some(_), None) => Err("side: a state dump needs `mode = polling' -- the application \
                                runs the callback from run_pending_statedumps() -- or \
                                `mode = agent_thread' -- a thread libside spawns runs it"
            .into()),
        (None, Some(_)) => {
            Err("side: `mode' says who runs a state dump callback, and no `statedump' \
                 names one"
                .into())
        }
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

struct EventField {
    name: String,
    kind: FieldKind,
    /// The attributes of the field's type, as they were written.
    attributes: String,
}

struct Event {
    name: String,
    provider: String,
    event: String,
    level: String,
    fields: Vec<EventField>,
    attributes: String,
}

fn events_impl(item: TokenStream, statedump: Option<&GroupStatedump>) -> Result<String, String> {
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
        group_body(&name, &events, statedump)
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
        attributes: String::new(),
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
            "attributes" => {
                let TokenTree::Group(group) = &part[colon + 1] else {
                    return Err("side: the attributes of an event go in brackets".into());
                };
                event.attributes = group.stream().to_string();
            }
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
                    let (kind, attributes) = parse_field(&field[colon + 1..])?;
                    event.fields.push(EventField {
                        name: name.to_string(),
                        kind,
                        attributes,
                    });
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

/// A field is its type and, where it has them, the attributes which
/// follow it in brackets.
///
/// A bracket group at the end is that list, unless it is the whole of
/// what was written, which makes it an array type instead. An event
/// field cannot be an array -- FieldType is not implemented for one --
/// so nothing is lost by reading it that way round.
fn parse_field(tokens: &[TokenTree]) -> Result<(FieldKind, String), String> {
    if tokens.len() > 1 {
        if let Some(TokenTree::Group(group)) = tokens.last() {
            if group.delimiter() == Delimiter::Bracket {
                return Ok((
                    parse_field_kind(&tokens[..tokens.len() - 1])?,
                    group.stream().to_string(),
                ));
            }
        }
    }
    Ok((parse_field_kind(tokens)?, String::new()))
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

fn group_body(name: &str, events: &[Event], statedump: Option<&GroupStatedump>) -> String {
    /* Each structure described elsewhere, in order of first mention. */
    let mut targets: Vec<String> = Vec::new();
    for event in events {
        for field in &event.fields {
            if let FieldKind::Extern(path) = &field.kind {
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
            .map(|field| {
                let (name, kind) = (&field.name, &field.kind);
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
                format!(
                    "::libside::side::Field {{ name: \"{name}\", layout: {layout}, attributes: &[{}] }},",
                    field.attributes
                )
            })
            .collect::<String>();
        field_lists.push_str(&format!(
            "const FIELDS_{i}: &[::libside::side::Field] = &[{fields}];"
        ));
        specs.push_str(&format!(
            "::libside::side::EventSpec {{ provider: {}, event: {}, loglevel: {}, flags: 0, fields: FIELDS_{i}, attributes: &[{}] }},",
            event.provider, event.event, event.level, event.attributes
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
                enabled: ::core::sync::atomic::AtomicUsize::new(0),
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
            .map(|field| (field.name.clone(), argument_type(&field.kind)))
            .collect::<Vec<_>>();
        let signature = arguments
            .iter()
            .map(|(name, type_)| format!("{name}: {type_},"))
            .collect::<String>();
        let name = &event.name;
        calls.push_str(&format!(
            r#"
        /// The two halves of this event, which `side_event!()` reaches
        /// by the path it is given: whether anything is listening, and
        /// the emission itself.
        ///
        /// They are apart so that asking can come first, which is what
        /// lets a call site skip working out arguments nothing would
        /// read. Emit it with
        ///
        /// ```ignore
        /// side_event!(path::to::{name}, ...);
        /// ```
        ///
        /// or, where the answer is wanted for something else, ask with
        /// `enabled()` and emit with `emit()`.
        #[allow(non_snake_case)]
        pub mod {name} {{
            use super::*;

            /// Whether a tracer is listening for this event.
            #[inline(always)]
            pub fn enabled() -> bool {{
                /*
                 * Relaxed, which is what side_event_enabled() reads it
                 * with: nothing is ordered against it, and the only
                 * thing asked of the compiler is that it read it here
                 * rather than remember what it held.
                 */
                let enabled = unsafe {{
                    &(*::core::ptr::addr_of_mut!(__side::STATE_{i})).enabled
                }};
                enabled.load(::core::sync::atomic::Ordering::Relaxed) != 0
            }}

            /// Emit it, without asking again.
            ///
            /// Inlined, into the unlikely half of the branch
            /// `side_event!()` writes around it.
            #[inline(always)]
            pub fn emit({signature}) {{
                let state = unsafe {{
                    ::core::ptr::addr_of!(
                        (*::core::ptr::addr_of_mut!(__side::STATE_{i})).parent)
                }};
                {emission}
            }}

            /// Emit it to the one tracer which asked for the state dump
            /// `key` identifies, rather than to every tracer listening.
            ///
            /// For a state dump callback, which `side_statedump_event!()`
            /// reaches this through. Emitting it outside one is what the
            /// key makes impossible: there is nowhere else to get one.
            #[inline(always)]
            pub fn emit_statedump(
                key: ::libside::side::StatedumpKey<'_>,
                {signature}
            ) {{
                let state = unsafe {{
                    ::core::ptr::addr_of!(
                        (*::core::ptr::addr_of_mut!(__side::STATE_{i})).parent)
                }};
                {statedump_emission}
            }}
        }}
"#,
            emission = with_side_args(
                &arguments,
                "unsafe { ::libside::side::call(state, &[{args}]); }"
            ),
            statedump_emission = with_side_args(
                &arguments,
                "unsafe { ::libside::side::statedump_call(state, &[{args}], key); }"
            ),
        ));
    }

    let count = events.len();

    /*
     * One hole per reference to a structure described elsewhere, in the
     * order the fields needing them were written, which is the order
     * the builder lays them out in and asserts they come out in.
     */
    let uses = events
        .iter()
        .flat_map(|event| &event.fields)
        .filter_map(|field| match &field.kind {
            FieldKind::Extern(path) => Some(path.clone()),
            FieldKind::Rust(_) => None,
        })
        .collect::<Vec<_>>();
    let nuses = uses.len();
    let (holes, desc_fields, desc_init) = description_object(&uses);
    let (statedump_items, statedump_register, statedump_unregister, statedump_calls) =
        match statedump {
            None => (String::new(), String::new(), String::new(), String::new()),
            Some(statedump) => group_statedump(name, statedump),
        };

    format!(
        r#"
        #[doc(hidden)]
        #[allow(non_snake_case, non_upper_case_globals)]
        pub mod __side {{
            use super::*;

            {field_lists}

            pub const SPECS: &[::libside::side::EventSpec] = &[{specs}];
            pub const SIZE: usize = ::libside::side::group_size(SPECS);

            /* One hole per reference to a structure described elsewhere. */
            pub const NR_PATCHES: usize = {nuses};

            const BUILT: ::libside::side::Built<SIZE, NR_PATCHES> =
                ::libside::side::build_group::<SIZE, NR_PATCHES>(SPECS);

            {holes}

            /*
             * Every description of the group in one object, sharing one
             * copy of each type they describe the same way.
             *
             * The bytes the const evaluator laid out, cut at every
             * address it could not write, with a pointer in its place:
             * written as a pointer it is a relocation, which the loader
             * fills in and which a reader of the file can follow.
             * Packed, so that nothing is inserted between the two.
             */
            #[repr(C, packed)]
            pub struct GroupDesc {{
                {desc_fields}
            }}

            const _: () = assert!(
                ::core::mem::size_of::<GroupDesc>() == SIZE,
                "side: the description object is not the size of the description"
            );

            #[used]
            #[cfg_attr(any(target_os = "linux", target_os = "android"), link_section = "side_event_description")]
            pub static mut DESC: GroupDesc = GroupDesc {{
                {desc_init}
            }};

            {states}

            #[used]
            #[cfg_attr(any(target_os = "linux", target_os = "android"), link_section = "side_event_state_ptr")]
            static mut STATE_PTRS: [*mut ::libside::side::SideEventState; {count}] = [{state_ptrs}];

            static mut HANDLE: *mut ::libside::side::SideEventsRegisterHandle =
                ::core::ptr::null_mut();

            {statedump_items}

            unsafe extern "C" fn register() {{
                unsafe {{
                    HANDLE = ::libside::side::side_events_register(
                        ::core::ptr::addr_of_mut!(STATE_PTRS).cast(), {count});
                    {statedump_register}
                }}
            }}

            unsafe extern "C" fn unregister() {{
                unsafe {{
                    {statedump_unregister}
                    ::libside::side::side_events_unregister(HANDLE);
                }}
            }}

            #[used]
            #[cfg_attr(any(target_os = "linux", target_os = "android"), link_section = ".init_array")]
            static REGISTER: unsafe extern "C" fn() = register;

            #[used]
            #[cfg_attr(any(target_os = "linux", target_os = "android"), link_section = ".fini_array")]
            static UNREGISTER: unsafe extern "C" fn() = unregister;
        }}

        {statedump_calls}

        {calls}
"#
    )
}

/// What a group's state dump adds: the callback libside can call, the
/// registration, and -- in polling mode -- the two calls the
/// application owes.
///
/// Returns what to declare inside `__side`, what `register()` and
/// `unregister()` add, and what the group module itself gains.
fn group_statedump(name: &str, statedump: &GroupStatedump) -> (String, String, String, String) {
    let GroupStatedump { callback, mode } = statedump;

    let items = format!(
        r#"
            /*
             * The callback, given a type here rather than left to be
             * checked where libside calls it: a signature which is
             * wrong is then a mismatch which names the function and
             * says what was wanted of it.
             */
            const _: fn(::libside::side::StatedumpKey<'_>) = {callback};

            pub static mut STATEDUMP_HANDLE:
                *mut ::libside::side::SideStatedumpRequestHandle =
                    ::core::ptr::null_mut();

            /*
             * libside hands a state dump callback a key and nothing of
             * the caller's, so the callback cannot be a closure. It
             * needs to capture nothing: which group this is, is known
             * here.
             */
            unsafe extern "C" fn statedump_callback(key: *mut ::core::ffi::c_void) {{
                /*
                 * The key is good until this returns and no longer,
                 * which is what the branded key says: it borrows from
                 * this call, so the callback cannot keep it.
                 */
                {callback}(unsafe {{ ::libside::side::StatedumpKey::from_raw(key) }});
            }}
"#
    );

    /*
     * After the events, and not before. Registering the callback queues
     * a state dump at once -- and in agent thread mode waits for it to
     * be run before returning -- so an event this group dumps would be
     * missed by the very first dump if it were not registered yet.
     * Both happen here, in one constructor, because the order of a
     * group's own .init_array entry against anything else's is not
     * defined.
     */
    let register = format!(
        r#"
                    STATEDUMP_HANDLE =
                        ::libside::side::side_statedump_request_notification_register(
                            b"{name} ".as_ptr().cast(),
                            statedump_callback,
                            ::libside::side::StatedumpMode::{mode});
"#
    );

    /* The reverse: the callback goes before the events it dumps. */
    let unregister = r#"
                    if !STATEDUMP_HANDLE.is_null() {
                        ::libside::side::side_statedump_request_notification_unregister(
                            STATEDUMP_HANDLE);
                        STATEDUMP_HANDLE = ::core::ptr::null_mut();
                    }
"#
    .to_string();

    /*
     * Only a polling handle answers these: libside returns false and
     * SIDE_ERROR_INVAL for an agent thread one, so writing them only
     * for the mode which has them makes the choice of mode a thing the
     * compiler knows rather than a runtime error.
     */
    let calls = if mode.as_str() == "Polling" {
        r#"
        /// Whether a tracer has asked this group for a state dump.
        ///
        /// The group was declared `mode = polling', so the application
        /// is what runs its callback, and this is how it knows to.
        pub fn statedump_pending() -> bool {
            unsafe {
                /*
                 * Nothing was asked of a group which never registered
                 * -- libside was finalized, or out of memory -- and
                 * asking libside about a handle it did not give us
                 * would be reading through a null pointer.
                 */
                if __side::STATEDUMP_HANDLE.is_null() {
                    return false;
                }
                ::libside::side::side_statedump_poll_pending_requests(
                    __side::STATEDUMP_HANDLE)
            }
        }

        /// Run the group's state dump callback for every tracer which
        /// has asked, on this thread, now.
        ///
        /// The obligation `mode = polling' takes on: a tracer which
        /// asks waits until this is reached.
        pub fn run_pending_statedumps() {
            unsafe {
                if __side::STATEDUMP_HANDLE.is_null() {
                    return;
                }
                /*
                 * The only thing this reports is a handle of the wrong
                 * mode, which the handle above cannot be: it is written
                 * for a polling group and no other.
                 */
                ::libside::side::side_statedump_run_pending_requests(
                    __side::STATEDUMP_HANDLE);
            }
        }
"#
        .to_string()
    } else {
        String::new()
    };

    (items, register, unregister, calls)
}

/// The description as an object: the runs of bytes the const evaluator
/// laid out, and a pointer at each hole it left.
///
/// Returns what to declare before the object, what it holds, and how it
/// is built. A group with no reference to a structure described
/// elsewhere is one run and nothing else.
fn description_object(uses: &[String]) -> (String, String, String) {
    let mut holes = String::new();
    let mut fields = String::new();
    let mut init = String::new();

    for (i, path) in uses.iter().enumerate() {
        holes.push_str(&format!("const HOLE_{i}: usize = BUILT.patches[{i}].at;\n"));
        if i == 0 {
            holes.push_str("const RUN_0: usize = HOLE_0;\n");
            init.push_str(
                "run_0: ::libside::side::description_run::<RUN_0>(&BUILT.bytes, 0),",
            );
        } else {
            holes.push_str(&format!(
                "const RUN_{i}: usize = HOLE_{i} - HOLE_{p} - ::libside::side::PATCH_WIDTH;\n",
                p = i - 1
            ));
            init.push_str(&format!(
                "run_{i}: ::libside::side::description_run::<RUN_{i}>(&BUILT.bytes, HOLE_{p} + ::libside::side::PATCH_WIDTH),",
                p = i - 1
            ));
        }
        fields.push_str(&format!(
            "run_{i}: [u8; RUN_{i}], ptr_{i}: ::libside::side::SideRawPtr,"
        ));
        init.push_str(&format!(
            "ptr_{i}: ::libside::side::SideRawPtr::from_ptr(unsafe {{ ::core::ptr::addr_of_mut!({path}::DESC) }}.cast()),"
        ));
    }

    let last = uses.len();
    if last == 0 {
        holes.push_str("const RUN_0: usize = SIZE;\n");
        init.push_str("run_0: ::libside::side::description_run::<RUN_0>(&BUILT.bytes, 0),");
    } else {
        holes.push_str(&format!(
            "const RUN_{last}: usize = SIZE - HOLE_{p} - ::libside::side::PATCH_WIDTH;\n",
            p = last - 1
        ));
        init.push_str(&format!(
            "run_{last}: ::libside::side::description_run::<RUN_{last}>(&BUILT.bytes, HOLE_{p} + ::libside::side::PATCH_WIDTH),",
            p = last - 1
        ));
    }
    fields.push_str(&format!("run_{last}: [u8; RUN_{last}],"));

    (holes, fields, init)
}

/// The nest of closures which keeps every argument alive until the call.
///
/// `call` is what to do once they all are, with `{args}` standing for
/// the arguments as libside wants them.
fn with_side_args(arguments: &[(String, String)], call: &str) -> String {
    let mut body = call.replace(
        "{args}",
        &arguments
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>()
            .join(", "),
    );
    for (name, type_) in arguments.iter().rev() {
        body = format!(
            "<{type_} as ::libside::side::FieldType>::with_side_arg({name}, |{name}| {{ {body} }})"
        );
    }
    body
}
