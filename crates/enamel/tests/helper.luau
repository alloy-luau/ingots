-- The state helper against a mock of the Roblox pieces it touches. The
-- test in emit.rs puts the helper above this file and runs it under
-- `luau`. An error fails the test.

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

local function Instance(class, props)
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
		local stroke = Instance("UIStroke", { Thickness = 0, Color = 0 })
		local scale = Instance("UIScale", { Scale = 1 })
		local el = Instance("TextButton", {}):Add(stroke):Add(scale)
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
	end
end

if #failures > 0 then
	error(table.concat(failures, "\n"))
end
