-- The tag helper against a mock of the instance it touches. The test in
-- emit.rs puts the helper above this file and runs it under `luau`. An
-- error fails the test.

local function signal()
	local s = { fns = {} }
	function s:Connect(f)
		table.insert(self.fns, f)
		return { Disconnect = function() end }
	end
	function s:Fire(...)
		for _, f in self.fns do
			f(...)
		end
	end
	return s
end

local INSTANCE = {}
INSTANCE.__index = INSTANCE

function INSTANCE:AddTag(tag)
	self.tags[tag] = true
end

function INSTANCE:GetPropertyChangedSignal(k)
	self.changed[k] = self.changed[k] or signal()
	return self.changed[k]
end

typeof = function(v)
	return if getmetatable(v) == INSTANCE then "Instance" else type(v)
end

local function new()
	return setmetatable({ tags = {}, changed = {}, Text = "", Activated = signal() }, INSTANCE)
end

local function type_text(el, text)
	el.Text = text
	el.changed.Text:Fire()
end

-- The element form: React calls a new ref on each render. The helper
-- connects once and runs the latest handler, and the chained ref runs.
local el = new()
local got = {}
local fits = 0
local first = __silk(nil, "a b", nil, function(text)
	table.insert(got, "first " .. text)
end, nil, function()
	fits += 1
end)
first(el)
local second = __silk(nil, "a b", nil, function(text)
	table.insert(got, "second " .. text)
end, 3, nil)
second(el)

assert(el.tags.a and el.tags.b, "the tags")
assert(#el.changed.Text.fns == 1, "one listener over two renders")
assert(fits == 1, "the chained ref ran")

type_text(el, "hi")
assert(got[1] == "second hi" and #got == 1, table.concat(got, ", "))

-- `maxLength` cuts the text to the latest limit.
type_text(el, "hello")
assert(el.Text == "hel", el.Text)

-- React calls the ref with nil when the element goes.
second(nil)

-- The table form builds the element and returns it.
local built = __silk(function()
	return new()
end, "x", nil)
assert(built.tags.x, "the table form tags")

print("ok")
