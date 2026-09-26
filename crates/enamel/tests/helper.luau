-- The state helper and the child component against a mock of the
-- Roblox pieces they touch. The test in emit.rs puts both above this
-- file and runs it under `luau`. An error fails the test.

-- In deferred mode a signal fires at flush(), as Roblox's default does.
local deferred = false
local queue = {}

local function flush()
	while #queue > 0 do
		local q = queue
		queue = {}
		for _, f in q do
			f()
		end
	end
end

local function signal()
	local s = { fns = {} }
	function s:Connect(f)
		table.insert(self.fns, f)
		return { Disconnect = function() end }
	end
	function s:Fire()
		for _, f in self.fns do
			if deferred then
				table.insert(queue, f)
			else
				f()
			end
		end
	end
	return s
end

-- Roblox stores these as float32, so a read gives less than the write.
local FLOATS = { BackgroundTransparency = true, Scale = true, Thickness = true }

local function make(class, props)
	local store, children, changed = {}, {}, {}
	local api = {
		ClassName = class,
		MouseEnter = signal(),
		MouseLeave = signal(),
		MouseButton1Down = signal(),
		MouseButton1Up = signal(),
		Focused = signal(),
		FocusLost = signal(),
	}
	function api:FindFirstChildOfClass(c)
		for _, child in children do
			if child.ClassName == c then
				return child
			end
		end
		return nil
	end
	function api:GetPropertyChangedSignal(k)
		changed[k] = changed[k] or signal()
		return changed[k]
	end
	function api:SetAttribute() end
	function api:Add(child)
		table.insert(children, child)
		return self
	end
	local inst = setmetatable({}, {
		__index = function(_, k)
			if api[k] ~= nil then
				return api[k]
			end
			return store[k]
		end,
		__newindex = function(_, k, v)
			if FLOATS[k] then
				v = string.unpack("f", string.pack("f", v))
			end
			if store[k] ~= v then
				store[k] = v
				if changed[k] then
					changed[k]:Fire()
				end
			end
		end,
	})
	for k, v in props do
		inst[k] = v
	end
	return inst
end

-- A tween moves halfway at step() and to its goal at finish().
local playing = {}
local TweenService = {}
function TweenService:Create(o, _info, goal)
	local t = { Completed = signal(), o = o, goal = goal, from = {} }
	for k in goal do
		t.from[k] = o[k]
	end
	function t:Play()
		table.insert(playing, self)
	end
	return t
end

local function step()
	for _, t in playing do
		for k, v in t.goal do
			t.o[k] = (t.from[k] + v) / 2
		end
	end
end

local function finish()
	local all = playing
	playing = {}
	for _, t in all do
		for k, v in t.goal do
			t.o[k] = v
		end
		t.Completed:Fire()
	end
end

-- Every signal and tween of one event runs to its end.
local function settle()
	flush()
	finish()
	flush()
end

game = {
	GetService = function()
		return TweenService
	end,
}

Instance = {
	new = function(class)
		return make(class, {})
	end,
}

local failures = {}

local function check(label, got, want)
	if math.abs(got - want) > 1e-6 then
		table.insert(failures, `{label}: {got}, want {want}`)
	end
end

local TWEEN = { kind = "default", info = {} }

for _, mode in { "immediate", "deferred" } do
	for _, tween in { false, true } do
		deferred = mode == "deferred"
		local tag = `{mode}, {if tween then "tween" else "direct"}`

		-- hover:scale-110 hover:stroke-yellow-400 hover:ring-4: the helper
		-- sets the UIScale and the UIStroke children, and puts them back.
		local stroke = make("UIStroke", { Thickness = 0, Color = 0 })
		local scale = make("UIScale", { Scale = 1 })
		local el = make("TextButton", {}):Add(stroke):Add(scale)
		__enamel(
			el,
			{ hover = { UIScale = { Scale = 1.1 }, UIStroke = { Color = 5, Thickness = 4 } } },
			false,
			if tween then TWEEN else nil
		)
		el.MouseEnter:Fire()
		settle()
		check(`{tag}: hover scale`, scale.Scale, 1.1)
		check(`{tag}: hover thickness`, stroke.Thickness, 4)
		check(`{tag}: hover color`, stroke.Color, 5)
		el.MouseLeave:Fire()
		settle()
		check(`{tag}: rest scale`, scale.Scale, 1)
		check(`{tag}: rest thickness`, stroke.Thickness, 0)
		check(`{tag}: rest color`, stroke.Color, 0)

		-- A source drives BackgroundTransparency, and
		-- hover:bg-transparency-90 changes it. The effect writes the
		-- value of the source now and on each change, as Vide does.
		local picked = false
		local button = make("TextButton", {})
		local function effect()
			button.BackgroundTransparency = if picked then 0 else 1
		end
		effect()
		flush()
		__enamel(
			button,
			{ hover = { BackgroundTransparency = 0.9 } },
			false,
			if tween then TWEEN else nil
		)

		-- The source changes at rest, then the pointer comes and goes.
		picked = true
		effect()
		flush()
		button.MouseEnter:Fire()
		settle()
		check(`{tag}: hover over a source`, button.BackgroundTransparency, 0.9)
		button.MouseLeave:Fire()
		settle()
		check(`{tag}: rest after a change at rest`, button.BackgroundTransparency, 0)

		-- The source changes under the pointer, as a click does.
		picked = false
		effect()
		flush()
		button.MouseEnter:Fire()
		settle()
		picked = true
		effect()
		flush()
		button.MouseLeave:Fire()
		settle()
		check(`{tag}: rest after a change on hover`, button.BackgroundTransparency, 0)

		-- The steps of a tween are the helper's own writes.
		button.MouseEnter:Fire()
		flush()
		step()
		settle()
		button.MouseLeave:Fire()
		flush()
		step()
		settle()
		check(`{tag}: rest after a plain hover`, button.BackgroundTransparency, 0)
	end
end

-- The table form makes a child with Instance.new, not with the factory.
local padding = __enamel_child({ Class = "UIPadding", PaddingTop = 4 })
check("child class", if padding.ClassName == "UIPadding" then 1 else 0, 1)
check("child property", padding.PaddingTop, 4)
check("child keeps no Class", if padding.Class == nil then 1 else 0, 1)

if #failures > 0 then
	error(table.concat(failures, "\n"))
end
