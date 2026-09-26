-- The viewport helper against a mock of the camera and the instances it
-- touches. The test in emit.rs puts the helper above this file and runs
-- it under `luau`. An error fails the test.

local function signal()
	local s = { fns = {} }
	function s:Connect(f)
		local c = { on = true }
		function c.Disconnect()
			c.on = false
		end
		c.fn = f
		table.insert(self.fns, c)
		return c
	end
	function s:Fire()
		for _, c in self.fns do
			if c.on then
				c.fn()
			end
		end
	end
	return s
end

local camera = { ViewportSize = { X = 1920, Y = 1080 }, changed = signal() }
function camera:GetPropertyChangedSignal(k)
	assert(k == "ViewportSize", k)
	return self.changed
end

game = {
	GetService = function(_, name)
		assert(name == "Workspace", name)
		return { CurrentCamera = camera }
	end,
}

local INSTANCE = {}
INSTANCE.__index = INSTANCE
local made = {}

function INSTANCE:FindFirstChild(name)
	for _, o in made do
		if o.Parent == self and o.Name == name then
			return o
		end
	end
	return nil
end

local function new(class)
	local o = setmetatable({ ClassName = class, Name = class, Destroying = signal() }, INSTANCE)
	table.insert(made, o)
	return o
end

Instance = { new = new }
typeof = function(v)
	return if getmetatable(v) == INSTANCE then "Instance" else type(v)
end
UDim2 = {
	fromScale = function(x, y)
		return { X = x, Y = y }
	end,
}

local function near(a, b)
	assert(math.abs(a - b) < 1e-9, `{a} is not {b}`)
end

local function scales(el)
	local n = 0
	for _, o in made do
		if o.Parent == el and o.ClassName == "UIScale" then
			n += 1
		end
	end
	return n
end

-- The table form builds the body and fits it: 1920 by 1080 over 1280 by
-- 720 is a scale of 1.5, and the body is the screen over that scale.
local body = __silk_viewport(1280, 720, 0.5, 2, nil, function()
	return new("Frame")
end)
local scale = body:FindFirstChild("viewport")
near(scale.Scale, 1.5)
near(body.Size.X, 1 / 1.5)

-- The smaller side decides, and the bounds clamp the scale.
camera.ViewportSize = { X = 1280, Y = 360 }
camera.changed:Fire()
near(scale.Scale, 0.5)
camera.ViewportSize = { X = 8000, Y = 8000 }
camera.changed:Fire()
near(scale.Scale, 2)
near(body.Size.Y, 0.5)

-- A destroyed body stops following the camera.
body.Destroying:Fire()
camera.ViewportSize = { X = 1280, Y = 720 }
camera.changed:Fire()
near(scale.Scale, 2)

-- The element form: React calls the ref with the instance, again with a
-- new ref on each render, and with nil when the body goes.
local el = new("Frame")
local first = __silk_viewport(640, nil, nil, nil, nil)
first(el)
near(el:FindFirstChild("viewport").Scale, 2)
first(nil)
local second = __silk_viewport(640, nil, nil, nil, nil)
second(el)
assert(scales(el) == 1, "one UIScale over two renders")
camera.ViewportSize = { X = 320, Y = 720 }
camera.changed:Fire()
near(el:FindFirstChild("viewport").Scale, 0.5)
second(nil)
camera.ViewportSize = { X = 1280, Y = 720 }
camera.changed:Fire()
near(el:FindFirstChild("viewport").Scale, 0.5)

-- With no design size, `initial-scale` is the scale.
local zoomed = __silk_viewport(nil, nil, nil, nil, 1.25, function()
	return new("Frame")
end)
near(zoomed:FindFirstChild("viewport").Scale, 1.25)

print("ok")
