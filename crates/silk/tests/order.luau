-- The order helper against a mock of the GuiObjects it orders. The test
-- in emit.rs puts the helper above this file and runs it under `luau`.
-- An error fails the test.

local function signal()
	local s = { fns = {} }
	function s:Connect(f)
		table.insert(self.fns, f)
		return { Disconnect = function() end }
	end
	function s:Fire()
		for _, f in self.fns do
			f()
		end
	end
	return s
end

local GUI = {}

-- A property write fires its changed signal at once, as the immediate
-- signal mode does, so the helper must not loop on its own write.
GUI.__index = function(o, k)
	return rawget(GUI, k) or o.props[k]
end
GUI.__newindex = function(o, k, v)
	o.props[k] = v
	if o.changed[k] then
		o.changed[k]:Fire()
	end
end

function GUI:IsA(class)
	return class == "GuiObject"
end

function GUI:GetPropertyChangedSignal(k)
	self.changed[k] = self.changed[k] or signal()
	return self.changed[k]
end

typeof = function(v)
	return if getmetatable(v) == GUI then "Instance" else type(v)
end

local function item(order)
	local o = setmetatable({ props = { LayoutOrder = order }, changed = {} }, GUI)
	return o
end

-- Vide's `values` gives the items in hash order; each binds its index.
local a, b, c = item(1), item(2), item(3)
__silk_order({ c, a, b }, 2000)
assert(a.LayoutOrder == 2001, a.LayoutOrder)
assert(b.LayoutOrder == 2002, b.LayoutOrder)
assert(c.LayoutOrder == 2003, c.LayoutOrder)

-- The list moves: Vide writes the new index, and the item keeps its place.
c.LayoutOrder = 1
a.LayoutOrder = 3
assert(c.LayoutOrder == 2001, c.LayoutOrder)
assert(a.LayoutOrder == 2003, a.LayoutOrder)

-- The library runs the child again with the same items.
__silk_order({ a, b, c }, 2000)
assert(c.LayoutOrder == 2001 and b.LayoutOrder == 2002 and a.LayoutOrder == 2003)

-- An item that gives no order takes its place in the list.
local x, y = item(0), item(0)
__silk_order(function()
	return { x, y }
end, 3000)()
assert(x.LayoutOrder == 3000 and y.LayoutOrder == 3001)
__silk_order({ y, x }, 3000)
assert(y.LayoutOrder == 3000 and x.LayoutOrder == 3001)

-- One instance takes the place, whatever order it gives.
local one = item(7)
__silk_order(one, 4000)
assert(one.LayoutOrder == 4000)
