//! The edits: the `ClassName` attribute becomes properties, the open
//! tag gains layout children, and an element with a state variant wraps
//! in the helper call.

use alloy_ingot::Edit;

use crate::classes::{self, Class, Context, Element, Resolved};
use crate::markup::Found;

/// The plan for one element: what the transform writes for it.
pub struct Plan<'a> {
    pub found: &'a Found,
    pub element: Option<Element>,
    pub resolved: Resolved,
}

pub fn plan<'a>(found: &'a Found, ctx: &Context) -> Plan<'a> {
    let classes: Vec<Class> = found.classes.iter().map(|(c, _)| Class::parse(c)).collect();
    // Silk makes a ScrollingFrame of an HTML box with an overflow class.
    let scrolls = found.classes.iter().any(|(c, _)| {
        matches!(
            c.as_str(),
            "overflow-auto"
                | "overflow-scroll"
                | "overflow-y-auto"
                | "overflow-y-scroll"
                | "overflow-x-auto"
                | "overflow-x-scroll"
                | "scroll-x"
                | "scroll-y"
                | "scroll-xy"
        )
    });
    let element = Element::parse(&found.tag).map(|e| match e {
        Element::Frame if found.html && scrolls => Element::ScrollingFrame,

        e => e,
    });
    let resolved = element
        .map(|e| classes::resolve(e, &classes, ctx))
        .unwrap_or_default();

    Plan {
        found,
        element,
        resolved,
    }
}

impl Plan<'_> {
    pub fn uses_helper(&self) -> bool {
        !self.resolved.states.is_empty() || self.resolved.group
    }

    pub fn uses_theme(&self) -> bool {
        self.resolved.uses_theme
    }

    pub fn adds_children(&self) -> bool {
        !self.resolved.children.is_empty()
    }

    /// Whether the classes set an axis of a `Size` the tag also writes.
    /// The two merge in that attribute: an axis no class names keeps the
    /// element's own size.
    pub fn merges_size(&self) -> bool {
        let (x, y) = self.resolved.size;

        self.found.size_attr.is_some() && (x.is_some() || y.is_some())
    }

    /// The attribute text that replaces `ClassName="..."`.
    fn attributes(&self) -> String {
        self.resolved
            .props
            .iter()
            .filter(|(k, _)| k != "Size" || !self.merges_size())
            .map(|(k, v)| format!("{k}={{{v}}}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The children the classes add, as markup. `child` names the
    /// component of [`child_text`] in the table form: a `<UIPadding>` tag
    /// would lower to a factory call, and Vide types its factory over 19
    /// classes, with no UIPadding, UIStroke, or UIScale.
    fn children(&self, child: Option<&str>) -> String {
        self.resolved
            .children
            .iter()
            .map(|c| {
                let props = c
                    .props
                    .iter()
                    .map(|(k, v)| format!(" {k}={{{v}}}"))
                    .collect::<String>();

                match child {
                    Some(name) => format!("<{name} Class=\"{}\"{props} />", c.class),

                    None => format!("<{}{props} />", c.class),
                }
            })
            .collect()
    }

    /// The Luau table of the states the helper applies.
    fn states_table(&self) -> String {
        let states = self
            .resolved
            .states
            .iter()
            .map(|(state, props)| {
                let fields = props
                    .iter()
                    .map(|(k, v)| format!("{k} = {v}"))
                    .collect::<Vec<_>>()
                    .join(", ");

                format!("{state} = {{ {fields} }}")
            })
            .collect::<Vec<_>>()
            .join(", ");

        format!("{{ {states} }}")
    }

    /// The edits for this element in `source`. `helper` names the state
    /// function; `table` says the project lowers markup in the table form,
    /// and `compute` is the `compute` of its factory, for Fusion.
    pub fn edits(
        &self,
        source: &str,
        helper: &str,
        table: bool,
        compute: Option<&str>,
    ) -> Vec<Edit> {
        let f = self.found;
        let mut edits = Vec::new();

        if self.element.is_none() {
            return edits;
        }

        let attrs = self.attributes();
        edits.push(Edit::replace(f.attr.0 as u32, f.attr.1 as u32, attrs));

        if let Some((s, e)) = f.size_attr
            && self.merges_size()
        {
            let axis = |d: Option<classes::Dim>| d.map_or("nil".to_string(), |d| d.luau());
            let (x, y) = self.resolved.size;
            let derive = compute.map_or(String::new(), |c| format!(", {c}"));
            edits.push(Edit::insert(s as u32, format!("{helper}_size(")));
            edits.push(Edit::insert(
                e as u32,
                format!(", {}, {}{derive})", axis(x), axis(y)),
            ));
        }

        let child = format!("{helper}_child");
        let children = self.children(table.then_some(child.as_str()));

        if !children.is_empty() {
            match f.self_close {
                Some((s, e)) => {
                    edits.push(Edit::replace(
                        s as u32,
                        e as u32,
                        format!(">{children}</{}>", f.tag),
                    ));
                }

                None => match f
                    .text_hole
                    .filter(|_| self.element.is_some_and(Element::has_text))
                {
                    // The lone hole is the Text of a text element. Beside
                    // the children it would be a child, which the markup
                    // compiler rejects, so it becomes `Text={...}` where
                    // it stands: the `>` moves past it.
                    Some((s, e)) => {
                        let gap = &source[f.open_end - 1..s];
                        edits.push(Edit::replace(
                            (f.open_end - 1) as u32,
                            s as u32,
                            format!("{} Text=", "\n".repeat(gap.matches('\n').count())),
                        ));
                        edits.push(Edit::insert(e as u32, format!(">{children}")));
                    }

                    None => edits.push(Edit::insert(f.open_end as u32, children)),
                },
            }
        }

        if self.uses_helper() {
            let group = if self.resolved.group { "true" } else { "false" };
            let tween = self
                .resolved
                .transition
                .as_ref()
                .map(classes::Transition::luau);

            match table {
                true => {
                    let tween = tween.map_or(String::new(), |t| format!(", {t}"));
                    let (open, close) = if f.in_children { ("{", "}") } else { ("", "") };
                    edits.push(Edit::insert(f.start as u32, format!("{open}{helper}(")));
                    edits.push(Edit::insert(
                        f.end as u32,
                        format!(", {}, {group}{tween}){close}", self.states_table()),
                    ));
                }

                // A React element is no instance: React hands the instance
                // to the helper as a `ref`. A `ref` Silk wrote runs first.
                false => {
                    let open = &source[f.start..f.open_end];
                    let silk = open.find("{{ ref = ").map(|at| {
                        let from = f.start + at;

                        (from, crate::markup::skip_hole(source, from))
                    });
                    let after = silk.map_or("nil".to_string(), |(s, e)| {
                        source[s + "{{ ref = ".len()..e - 2].trim().to_string()
                    });
                    let reference = format!(
                        "{{{{ ref = {helper}(nil, {}, {group}, {}, {after}) }}}}",
                        self.states_table(),
                        tween.unwrap_or_else(|| "nil".into())
                    );

                    match silk {
                        Some((s, e)) => edits.push(Edit::replace(s as u32, e as u32, reference)),

                        None => edits.push(Edit::insert(f.attr.1 as u32, format!(" {reference}"))),
                    }
                }
            }
        }

        edits
    }
}

/// The helper as one line of Alloy, so the file keeps its line count.
/// A table value in a state holds the properties of a child, `UIScale`
/// or `UIStroke`, which the helper finds by its class.
///
/// `base` holds the rest value of each property a state changes. A
/// source, such as a Vide function or a Fusion state, can write the
/// property after the wrap. The helper watches the property, and a write
/// that is not its own becomes the rest value. `mine` holds the value
/// the helper wrote, as read back, because a float property keeps less
/// precision than the number written. `busy` covers the immediate signal
/// mode. A change while a tween moves the property comes from the tween.
// ponytail: a source write during a tween of the same property is lost,
// because the tween writes over it. Watch the tween steps to keep it.
pub fn helper_text(helper: &str) -> String {
    format!(
        "local {helper}_done: {{ [any]: boolean }} = setmetatable({{}}, {{ __mode = \"k\" }}) :: any \
local function {helper}(el: any, states: {{ hover: {{ [string]: any }}?, active: {{ [string]: any }}?, focus: {{ [string]: any }}?, group_hover: {{ [string]: any }}? }}, group: boolean, tween: {{ kind: string, info: TweenInfo }}?, after: any): any \
if el == nil then return function(value: any) if after ~= nil then local fit: any = after fit(value) end \
if typeof(value) == \"Instance\" and not {helper}_done[value] then {helper}_done[value] = true {helper}(value, states, group, tween) end end end \
local function targets(props: {{ [string]: any }}): {{ [any]: {{ [string]: any }} }} local out: {{ [any]: {{ [string]: any }} }} = {{ [el] = {{}} }} for k, v in props do if type(v) == \"table\" then local c = el:FindFirstChildOfClass(k) if c ~= nil then out[c] = v end else out[el][k] = v end end return out end \
local base: {{ [any]: {{ [string]: any }} }} = {{}} \
local mine: {{ [any]: {{ [string]: any }} }} = {{}} \
local moving: {{ [any]: {{ [string]: any }} }} = {{}} \
local busy = false \
local sets: {{ [string]: {{ [any]: {{ [string]: any }} }} }} = {{}} \
for name, props in states :: {{ [string]: {{ [string]: any }} }} do local set = targets(props) sets[name] = set for o, p in set do if base[o] == nil then base[o] = {{}} mine[o] = {{}} moving[o] = {{}} end local b, m, mv = base[o], mine[o], moving[o] for k in p do if b[k] == nil then b[k] = o[k] o:GetPropertyChangedSignal(k):Connect(function() if busy or mv[k] ~= nil then return end local v = o[k] if v ~= m[k] then b[k] = v end end) end end end end \
local function tweens(kind: string, k: string, v: any): boolean local t = typeof(v) if t ~= \"number\" and t ~= \"Color3\" and t ~= \"UDim2\" and t ~= \"UDim\" and t ~= \"Vector2\" and t ~= \"Vector3\" and t ~= \"Rect\" then return false end if kind == \"all\" then return true end local color = string.sub(k, -6) == \"Color3\" or k == \"Color\" local opacity = string.sub(k, -12) == \"Transparency\" local transform = k == \"Position\" or k == \"Size\" or k == \"Rotation\" or k == \"AnchorPoint\" or k == \"Scale\" if kind == \"colors\" then return color elseif kind == \"opacity\" then return opacity elseif kind == \"transform\" then return transform end return color or opacity or transform or k == \"Thickness\" end \
local function apply(set: {{ [any]: {{ [string]: any }} }}) local tw = tween for o, props in set do local m, mv = mine[o], moving[o] local goal: {{ [string]: any }} = {{}} local moves = false for k, v in props do if tw ~= nil and tweens(tw.kind, k, v) then goal[k] = v moves = true else busy = true o[k] = v busy = false m[k] = o[k] end end if moves and tw ~= nil then local t = game:GetService(\"TweenService\"):Create(o, tw.info, goal) for k in goal do mv[k] = t end t.Completed:Connect(function() for k in goal do if mv[k] == t then mv[k] = nil m[k] = o[k] end end end) t:Play() end end end \
local function reset() apply(base) end \
local hover = sets.hover if hover ~= nil then local h = hover el.MouseEnter:Connect(function() apply(h) end) el.MouseLeave:Connect(reset) end \
local active = sets.active if active ~= nil then local a = active el.MouseButton1Down:Connect(function() apply(a) end) el.MouseButton1Up:Connect(function() if hover ~= nil then apply(hover) else reset() end end) end \
local focus = sets.focus if focus ~= nil then local f = focus el.Focused:Connect(function() apply(f) end) el.FocusLost:Connect(reset) end \
if group then el:SetAttribute(\"enamel_group\", true) end \
local group_hover = sets.group_hover if group_hover ~= nil then local gh = group_hover task.defer(function() local g = el.Parent while g ~= nil and g:GetAttribute(\"enamel_group\") == nil do g = g.Parent end if g ~= nil then g.MouseEnter:Connect(function() apply(gh) end) g.MouseLeave:Connect(reset) end end) end \
return el end "
    )
}

/// The component that makes a child the classes add, in the table form.
/// Vide and Fusion build instances, so `Instance.new` makes the child and
/// no typed factory call names its class.
pub fn child_text(helper: &str) -> String {
    format!(
        "local function {helper}_child(props: any): Instance local c: any = Instance.new(props.Class) for k, v in props do if k ~= \"Class\" then c[k] = v end end return c end "
    )
}

/// The merge of a `Size` attribute with the axes the classes name. A
/// source stays a source: a function under Vide, a React binding, and a
/// Fusion state, which derives through the `compute` of the factory.
pub fn size_text(helper: &str) -> String {
    format!(
        "local function {helper}_size(size: any, x: UDim?, y: UDim?, compute: any?): any local function merge(s: UDim2): UDim2 return UDim2.new(x or s.X, y or s.Y) end local kind = type(size) if kind == \"function\" then local read: () -> UDim2 = size return function() return merge(read()) end end if kind == \"table\" and size.map ~= nil then local map: (any, (UDim2) -> UDim2) -> any = size.map return map(size, merge) end \
if kind == \"table\" and compute ~= nil and size.type == \"State\" then local derive: any = compute return derive(function(use: any) return merge(use(size)) end) end \
return merge(size) end "
    )
}

/// The factory in the `[alx]` table the host sends at init: whether the
/// project lowers markup in the table form, `create(name)(props)`, as
/// Vide and Fusion do, and the `compute` it sets for Fusion. An old host
/// sends none, and Enamel takes the element form, as Alloy does with no
/// `[alx]`.
pub fn factory(alx: &serde_json::Value) -> (bool, Option<String>) {
    let text = |key: &str| alx["factory"].get(key).and_then(serde_json::Value::as_str);

    (
        text("backend") == Some("table"),
        text("compute").map(str::to_string),
    )
}

/// The byte where the helper goes: the start of the first line that is
/// code, past the leading comments and blank lines.
pub fn helper_at(source: &str) -> u32 {
    let mut at = 0usize;

    loop {
        let rest = &source[at..];
        let trimmed = rest.trim_start();
        let comment = at + rest.len() - trimmed.len();

        if trimmed.is_empty() {
            return source.len() as u32;
        }

        // Past a comment: a line, or a long one to its close bracket.
        let Some(end) = crate::markup::comment_end(source, comment) else {
            // Back to the start of the line, so the helper opens it.
            let line_start = source[..comment].rfind('\n').map_or(0, |n| n + 1);

            return line_start.max(at) as u32;
        };
        at = source[end..]
            .find('\n')
            .map_or(source.len(), |n| end + n + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markup;

    #[test]
    fn the_helper_follows_a_block_comment() {
        let src = "--[[\n  a\n]]\n--!strict\nlocal x = 1\n";

        assert_eq!(&src[helper_at(src) as usize..], "local x = 1\n");
        assert_eq!(helper_at("local x = 1\n"), 0);
    }

    fn apply(source: &str, edits: &[Edit]) -> String {
        let mut edits: Vec<&Edit> = edits.iter().collect();
        edits.sort_by_key(|e| (e.0, e.1));
        let mut out = String::new();
        let mut cursor = 0usize;

        for e in edits {
            out.push_str(&source[cursor..e.0 as usize]);
            out.push_str(&e.2);
            cursor = e.1 as usize;
        }

        out.push_str(&source[cursor..]);

        out
    }

    #[test]
    fn a_self_closing_element_gains_children_and_properties() {
        let src = "local x = <Frame ClassName=\"flex gap-2 bg-red-500 rounded\" Name=\"a\" />\n";
        let found = markup::find(src);
        let plan = plan(&found[0], &Context::default());
        let out = apply(src, &plan.edits(src, "__enamel", false, None));
        assert_eq!(
            out,
            "local x = <Frame BackgroundColor3={Color3.fromRGB(239, 68, 68)} Name=\"a\" ><UIListLayout FillDirection={Enum.FillDirection.Horizontal} SortOrder={Enum.SortOrder.LayoutOrder} Padding={UDim.new(0, 8)} /><UICorner CornerRadius={UDim.new(0, 4)} /></Frame>\n"
        );
    }

    #[test]
    fn a_transition_rides_along_as_the_tween() {
        let src = "return (\n    <Frame>\n        <TextButton ClassName=\"bg-red-500 hover:bg-red-600 transition duration-300\">Go</TextButton>\n    </Frame>\n)\n";
        let found = markup::find(src);
        let plan = plan(&found[0], &Context::default());
        let out = apply(src, &plan.edits(src, "__enamel", true, None));
        assert!(out.contains(", false, { kind = \"default\", info = TweenInfo.new(0.3, Enum.EasingStyle.Quad, Enum.EasingDirection.InOut, 0, false, 0) })}"), "{out}");
        assert!(helper_text("__enamel").contains("TweenService"));
    }

    /// In the table form a child the classes add is the component of
    /// `child_text`, so no typed factory call names its class.
    #[test]
    fn the_table_form_makes_children_with_the_component() {
        let src = "return <TextLabel ClassName=\"p-2 rounded\" Text=\"a\" />\n";
        let found = markup::find(src);
        let plan = plan(&found[0], &Context::default());

        assert_eq!(
            apply(src, &plan.edits(src, "__enamel", true, None)),
            "return <TextLabel  Text=\"a\" ><__enamel_child Class=\"UIPadding\" PaddingBottom={UDim.new(0, 8)} PaddingLeft={UDim.new(0, 8)} PaddingRight={UDim.new(0, 8)} PaddingTop={UDim.new(0, 8)} /><__enamel_child Class=\"UICorner\" CornerRadius={UDim.new(0, 4)} /></TextLabel>\n"
        );
        assert!(apply(src, &plan.edits(src, "__enamel", false, None)).contains("<UIPadding "));

        let alx = serde_json::json!({ "factory": { "backend": "table", "compute": "computed" } });
        assert_eq!(factory(&alx), (true, Some("computed".into())));
        assert_eq!(factory(&serde_json::Value::Null), (false, None));
    }

    /// Silk makes a ScrollingFrame of an HTML box with an overflow class,
    /// so the class fits and reports nothing.
    #[test]
    fn an_overflow_class_on_an_html_box_fits() {
        let src = "return <div className=\"h-10 overflow-y-auto canvas-auto\"><p>a</p></div>\n";
        let found = markup::find_all(src);
        let plan = plan(&found[0], &Context::default());

        assert_eq!(plan.element, Some(Element::ScrollingFrame));
        assert!(
            plan.resolved.problems.is_empty(),
            "{:?}",
            plan.resolved.problems
        );
    }

    /// A lone hole in a text element is its Text. With children beside
    /// it, the hole moves into the open tag as `Text={...}`, in place, so
    /// the file keeps its lines. A tag that sets Text keeps a hole child.
    #[test]
    fn a_lone_hole_stays_the_text_beside_the_children() {
        let run = |src: &str| {
            let found = markup::find(src);

            apply(
                src,
                &plan(&found[0], &Context::default()).edits(src, "__enamel", false, None),
            )
        };

        assert_eq!(
            run("return <TextButton ClassName=\"rounded\">{x}</TextButton>\n"),
            "return <TextButton  Text={x}><UICorner CornerRadius={UDim.new(0, 4)} /></TextButton>\n"
        );
        assert_eq!(
            run("return (\n  <TextLabel ClassName=\"rounded\">\n    {x}\n  </TextLabel>\n)\n"),
            "return (\n  <TextLabel \n Text={x}><UICorner CornerRadius={UDim.new(0, 4)} />\n  </TextLabel>\n)\n"
        );
        assert_eq!(
            run("return <TextLabel ClassName=\"rounded\" Text=\"a\">{x}</TextLabel>\n"),
            "return <TextLabel  Text=\"a\"><UICorner CornerRadius={UDim.new(0, 4)} />{x}</TextLabel>\n"
        );
        // A Frame has no Text: its hole is a child.
        assert!(run("return <Frame ClassName=\"rounded\">{x}</Frame>\n").contains("/>{x}</Frame>"));
    }

    /// A `Size` attribute keeps the axis no class names; the class sets
    /// the other one inside it.
    #[test]
    fn a_size_attribute_keeps_the_axis_no_class_names() {
        let src = "return <Frame ClassName=\"h-10 bg-red-500\" Size={UDim2.new(1, 0, 0, 0)} />\n";
        let found = markup::find(src);
        let plan = plan(&found[0], &Context::default());

        assert!(plan.merges_size());
        assert_eq!(
            apply(src, &plan.edits(src, "__enamel", false, None)),
            "return <Frame BackgroundColor3={Color3.fromRGB(239, 68, 68)} Size={__enamel_size(UDim2.new(1, 0, 0, 0), nil, UDim.new(0, 40))} />\n"
        );

        // Under Fusion the merge derives a state through `compute`.
        assert!(
            apply(src, &plan.edits(src, "__enamel", true, Some("computed")))
                .contains("UDim.new(0, 40), computed)}")
        );

        // With no attribute, the classes write the whole Size.
        let src = "return <Frame ClassName=\"h-10\" />\n";
        let found = markup::find(src);
        assert!(!super::plan(&found[0], &Context::default()).merges_size());
    }

    /// The helper runs under `luau` against the mock element of
    /// `tests/helper.luau`. A machine without `luau` skips the run.
    #[test]
    fn the_helper_runs_against_a_mock_element() {
        let script = format!(
            "{}\n{}\n{}\n{}",
            helper_text("__enamel"),
            child_text("__enamel"),
            size_text("__enamel"),
            include_str!("../tests/helper.luau")
        );
        let path = std::env::temp_dir().join(format!("enamel-helper-{}.luau", std::process::id()));
        std::fs::write(&path, script).unwrap();
        let run = std::process::Command::new("luau").arg(&path).output();
        let _ = std::fs::remove_file(&path);
        let Ok(out) = run else {
            eprintln!("no luau on PATH: the helper run is skipped");

            return;
        };

        assert!(
            out.status.success(),
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn a_state_wraps_the_element_in_the_helper() {
        let src = "return (\n    <Frame>\n        <TextButton ClassName=\"bg-red-500 hover:bg-red-600\">Go</TextButton>\n    </Frame>\n)\n";
        let found = markup::find(src);
        let plan = plan(&found[0], &Context::default());
        let out = apply(src, &plan.edits(src, "__enamel", true, None));
        assert!(out.contains("{__enamel(<TextButton BackgroundColor3={Color3.fromRGB(239, 68, 68)}>Go</TextButton>, { hover = { BackgroundColor3 = Color3.fromRGB(220, 38, 38) } }, false)}"), "{out}");
        assert_eq!(out.matches('\n').count(), src.matches('\n').count());

        // React's element is no instance: the helper takes it as a `ref`.
        let out = apply(src, &plan.edits(src, "__enamel", false, None));
        assert!(out.contains("<TextButton BackgroundColor3={Color3.fromRGB(239, 68, 68)} {{ ref = __enamel(nil, { hover = { BackgroundColor3 = Color3.fromRGB(220, 38, 38) } }, false, nil, nil) }}>Go</TextButton>"), "{out}");

        // A `ref` that Silk wrote runs inside the one of Enamel.
        let src = "return <TextButton Name={\"b\"} {{ ref = __silk(nil, \"x\", nil, nil, nil, nil) }} ClassName=\"hover:bg-red-600\">Go</TextButton>\n";
        let found = markup::find(src);
        let out = apply(
            src,
            &super::plan(&found[0], &Context::default()).edits(src, "__enamel", false, None),
        );
        assert!(out.contains("{{ ref = __enamel(nil, { hover = { BackgroundColor3 = Color3.fromRGB(220, 38, 38) } }, false, nil, __silk(nil, \"x\", nil, nil, nil, nil)) }}"), "{out}");
        assert_eq!(out.matches("ref =").count(), 1, "{out}");
    }
}
