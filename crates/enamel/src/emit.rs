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
    let element = Element::parse(&found.tag);
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

    /// The attribute text that replaces `ClassName="..."`.
    fn attributes(&self) -> String {
        self.resolved
            .props
            .iter()
            .map(|(k, v)| format!("{k}={{{v}}}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The children the classes add, as markup.
    fn children(&self) -> String {
        self.resolved
            .children
            .iter()
            .map(|c| {
                let props = c
                    .props
                    .iter()
                    .map(|(k, v)| format!(" {k}={{{v}}}"))
                    .collect::<String>();

                format!("<{}{props} />", c.class)
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

    /// The edits for this element. `helper` names the state function.
    pub fn edits(&self, helper: &str) -> Vec<Edit> {
        let f = self.found;
        let mut edits = Vec::new();

        if self.element.is_none() {
            return edits;
        }

        let attrs = self.attributes();
        edits.push(Edit::replace(f.attr.0 as u32, f.attr.1 as u32, attrs));

        let children = self.children();

        if !children.is_empty() {
            match f.self_close {
                Some((s, e)) => {
                    edits.push(Edit::replace(
                        s as u32,
                        e as u32,
                        format!(">{children}</{}>", f.tag),
                    ));
                }

                None => edits.push(Edit::insert(f.open_end as u32, children)),
            }
        }

        if self.uses_helper() {
            let group = if self.resolved.group { "true" } else { "false" };
            let (open, close) = if f.in_children { ("{", "}") } else { ("", "") };
            edits.push(Edit::insert(f.start as u32, format!("{open}{helper}(")));
            edits.push(Edit::insert(
                f.end as u32,
                format!(", {}, {group}){close}", self.states_table()),
            ));
        }

        edits
    }
}

/// The helper as one line of Alloy, so the file keeps its line count.
pub fn helper_text(helper: &str) -> String {
    format!(
        "local function {helper}(el: any, states: {{ hover: {{ [string]: any }}?, active: {{ [string]: any }}?, focus: {{ [string]: any }}?, group_hover: {{ [string]: any }}? }}, group: boolean): any \
local base: {{ [string]: any }} = {{}} \
for _, props in states :: {{ [string]: {{ [string]: any }} }} do for k in props do if base[k] == nil then base[k] = el[k] end end end \
local function apply(props: {{ [string]: any }}) for k, v in props do el[k] = v end end \
local function reset() apply(base) end \
local hover = states.hover if hover ~= nil then local h = hover el.MouseEnter:Connect(function() apply(h) end) el.MouseLeave:Connect(reset) end \
local active = states.active if active ~= nil then local a = active el.MouseButton1Down:Connect(function() apply(a) end) el.MouseButton1Up:Connect(function() if hover ~= nil then apply(hover) else reset() end end) end \
local focus = states.focus if focus ~= nil then local f = focus el.Focused:Connect(function() apply(f) end) el.FocusLost:Connect(reset) end \
if group then el:SetAttribute(\"enamel_group\", true) end \
local group_hover = states.group_hover if group_hover ~= nil then local gh = group_hover task.defer(function() local g = el.Parent while g ~= nil and g:GetAttribute(\"enamel_group\") == nil do g = g.Parent end if g ~= nil then g.MouseEnter:Connect(function() apply(gh) end) g.MouseLeave:Connect(reset) end end) end \
return el end "
    )
}

/// The byte where the helper goes: the start of the first line that is
/// code, past the leading comments and blank lines.
pub fn helper_at(source: &str) -> u32 {
    let mut at = 0usize;

    for line in source.split_inclusive('\n') {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with("--") {
            at += line.len();

            continue;
        }

        break;
    }

    at.min(source.len()) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markup;

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
        let out = apply(src, &plan.edits("__enamel"));
        assert_eq!(
            out,
            "local x = <Frame BackgroundColor3={Color3.fromRGB(239, 68, 68)} Name=\"a\" ><UIListLayout FillDirection={Enum.FillDirection.Horizontal} SortOrder={Enum.SortOrder.LayoutOrder} Padding={UDim.new(0, 8)} /><UICorner CornerRadius={UDim.new(0, 4)} /></Frame>\n"
        );
    }

    #[test]
    fn a_state_wraps_the_element_in_the_helper() {
        let src = "return (\n    <Frame>\n        <TextButton ClassName=\"bg-red-500 hover:bg-red-600\">Go</TextButton>\n    </Frame>\n)\n";
        let found = markup::find(src);
        let plan = plan(&found[0], &Context::default());
        let out = apply(src, &plan.edits("__enamel"));
        assert!(out.contains("{__enamel(<TextButton BackgroundColor3={Color3.fromRGB(239, 68, 68)}>Go</TextButton>, { hover = { BackgroundColor3 = Color3.fromRGB(220, 38, 38) } }, false)}"), "{out}");
        assert_eq!(out.matches('\n').count(), src.matches('\n').count());
    }
}
