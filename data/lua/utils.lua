-- General/miscellaneous helpers.
-- Pre-exported globally in all Lua contexts.

--[[

Copyright (C) 2022 ***REMOVED***

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program.  If not, see <http://www.gnu.org/licenses/>.

]]

--- @type serpentlib
local serpent = import('/impure/lua/serpent.lua')

function _G.repr(obj)
	return serpent.block(obj, {
		indent = '\t',
		maxlevel = 8,
		nocode = true,
		numformat = '%.6g'
	})
end