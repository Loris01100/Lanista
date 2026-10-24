-- Draws a first gladiator on the open art/fighters/gladiateur.aseprite (made by
-- fighter_template.lua with the look `gladiateur`), a base to redraw by hand: crested
-- bronze helmet with a pierced visor, bare scarred torso, leather belt and short
-- loincloth, a manica on the sword arm, a short sword, a shield or a second sword,
-- bronze greaves and leather sandals. Tinted layers are drawn in light grey (the
-- game multiplies them by the fighter's tints: skin, and the club's for the loincloth
-- and the crest), metal, leather and wood in their own colours, in the `_detail`
-- layers or the plain layers (weapon, weapon_back, shield).
--
-- The 16 frames follow docs/Formats_graphiques.md (idle, move, attack, death) and the
-- head's movement in data/fighter.ron (`head_frames`): the upper body moves like the
-- head, and in death the whole body turns around the neck as the head does, so that
-- the head the game moves stays on the body. Head layers are drawn on the first frame
-- only, as the game uses that one alone.
--
-- In Aseprite: open art/fighters/gladiateur.aseprite, then run it from File > Scripts.
-- Its drawing replaces what the layers it draws hold (undo with Ctrl+Z).

local FRAME = 32

-- `head_pivot` and `head_frames` of data/fighter.ron: the neck, and for each frame the
-- head's move (right, down) and turn (degrees, clockwise).
local NECK = { 16, 12 }
local HEAD_FRAMES = {
  { 0, 0, 0 }, { 0, 1, 0 }, { 0, 1, 0 }, { 0, 0, 0 },
  { 0, 0, 0 }, { 0, 1, 0 }, { 0, 0, 0 }, { 0, 1, 0 },
  { -1, 0, 0 }, { 0, 0, 0 }, { 1, 0, 0 }, { 0, 0, 0 },
  { -2.7, 0.1, -22.5 }, { -4.6, 2.4, -45 }, { -4.9, 7.3, -67.5 }, { -3, 12, -90 },
}

-- Feet (back, front), hands (back, front) before the upper body's move, and the
-- sword's angle in degrees (0 points ahead, negative up), for frames 1 to 12; death
-- frames take the first one.
local POSES = {
  -- idle
  { feet = { { 13, 30 }, { 19, 30 } }, hands = { { 21, 17 }, { 21, 19 } }, sword = -35 },
  { feet = { { 13, 30 }, { 19, 30 } }, hands = { { 21, 17 }, { 21, 19 } }, sword = -35 },
  { feet = { { 13, 30 }, { 19, 30 } }, hands = { { 21, 17 }, { 21, 19 } }, sword = -35 },
  { feet = { { 13, 30 }, { 19, 30 } }, hands = { { 21, 17 }, { 21, 19 } }, sword = -35 },
  -- move
  { feet = { { 11, 30 }, { 20, 30 } }, hands = { { 21, 17 }, { 20, 19 } }, sword = -30 },
  { feet = { { 15, 29 }, { 17, 30 } }, hands = { { 21, 17 }, { 21, 19 } }, sword = -35 },
  { feet = { { 20, 30 }, { 12, 30 } }, hands = { { 21, 18 }, { 22, 19 } }, sword = -40 },
  { feet = { { 17, 30 }, { 15, 29 } }, hands = { { 21, 17 }, { 21, 19 } }, sword = -35 },
  -- attack: wind up, swing, thrust (the impact), recover
  { feet = { { 13, 30 }, { 19, 30 } }, hands = { { 20, 16 }, { 15, 12 } }, sword = -150 },
  { feet = { { 13, 30 }, { 20, 30 } }, hands = { { 21, 16 }, { 20, 12 } }, sword = -70 },
  { feet = { { 12, 30 }, { 22, 30 } }, hands = { { 20, 17 }, { 25, 16 } }, sword = 0 },
  { feet = { { 13, 30 }, { 20, 30 } }, hands = { { 21, 17 }, { 23, 18 } }, sword = -20 },
}
-- The second sword's angle, held low.
local BACK_SWORD = -40

local function rgb(r, g, b)
  return app.pixelColor.rgba(r, g, b, 255)
end

-- Greys of the tinted layers: the tint at full on LIGHT, darker below.
local LIGHT, MID, SHADE, LINE = rgb(240, 240, 240), rgb(208, 208, 208),
    rgb(168, 168, 168), rgb(96, 96, 96)
local BRONZE = { hi = rgb(226, 178, 92), mid = rgb(186, 130, 54), dark = rgb(130, 86, 36),
  line = rgb(74, 46, 20) }
local STEEL = { hi = rgb(240, 244, 250), mid = rgb(188, 194, 206), line = rgb(58, 62, 74) }
local LEATHER = { mid = rgb(146, 94, 54), dark = rgb(102, 62, 34), line = rgb(62, 38, 22) }
local WOOD = { mid = rgb(156, 102, 58), dark = rgb(110, 70, 38) }
local MANICA = { cloth = rgb(226, 212, 176), band = rgb(170, 150, 112) }
local SCAR = rgb(184, 104, 96)
local HOLE = rgb(30, 22, 16)

local function round(v)
  return math.floor(v + 0.5)
end

local function put(image, x, y, color)
  if x >= 0 and y >= 0 and x < FRAME and y < FRAME then
    image:drawPixel(x, y, color)
  end
end

local function rect(image, x0, y0, x1, y1, color)
  for y = y0, y1 do
    for x = x0, x1 do
      put(image, x, y, color)
    end
  end
end

local function disk(image, cx, cy, radius, color)
  for y = 0, FRAME - 1 do
    for x = 0, FRAME - 1 do
      local dx, dy = x + 0.5 - cx, y + 0.5 - cy
      if dx * dx + dy * dy <= radius * radius then
        image:drawPixel(x, y, color)
      end
    end
  end
end

-- Squares of `size` px along the segment from `a` to `b`; `colors` alternate.
local function line(image, a, b, size, colors)
  if type(colors) ~= "table" then
    colors = { colors }
  end
  local dx, dy = b[1] - a[1], b[2] - a[2]
  local steps = math.max(math.abs(dx), math.abs(dy), 1)
  for i = 0, steps do
    local x, y = round(a[1] + dx * i / steps), round(a[2] + dy * i / steps)
    rect(image, x, y, x + size - 1, y + size - 1, colors[i % #colors + 1])
  end
end

local function is_drawn(image, x, y)
  return x >= 0 and y >= 0 and x < FRAME and y < FRAME
      and app.pixelColor.rgbaA(image:getPixel(x, y)) > 0
end

-- A 1 px line around what the image holds.
local function outline(image, color)
  local marks = {}
  for y = 0, FRAME - 1 do
    for x = 0, FRAME - 1 do
      if not is_drawn(image, x, y) and (is_drawn(image, x - 1, y)
            or is_drawn(image, x + 1, y) or is_drawn(image, x, y - 1)
            or is_drawn(image, x, y + 1)) then
        table.insert(marks, { x, y })
      end
    end
  end
  for _, mark in ipairs(marks) do
    image:drawPixel(mark[1], mark[2], color)
  end
end

local function add(point, dx, dy)
  return { point[1] + dx, point[2] + dy }
end

-- The joints of frame `number`, death frames standing as the first one.
local function pose(number)
  local still = number > #POSES
  local base = POSES[still and 1 or number]
  local move = HEAD_FRAMES[still and 1 or number]
  local bx, by = round(move[1]), round(move[2])
  return {
    bx = bx,
    by = by,
    shoulder_back = { 14 + bx, 13 + by },
    shoulder_front = { 18 + bx, 13 + by },
    hip_back = { 15, 21 + by },
    hip_front = { 17, 21 + by },
    foot_back = base.feet[1],
    foot_front = base.feet[2],
    hand_back = add(base.hands[1], bx, by),
    hand_front = add(base.hands[2], bx, by),
    sword = base.sword,
  }
end

-- A bent limb's middle joint: the elbow behind and below, the knee ahead.
local function elbow(shoulder, hand)
  return { round((shoulder[1] + hand[1]) / 2) - 1, round((shoulder[2] + hand[2]) / 2) + 1 }
end

local function knee(hip, foot)
  local dx, dy = foot[1] - hip[1], foot[2] - hip[2]
  local bend = 1 + math.max(0, 9 - math.sqrt(dx * dx + dy * dy))
  return { round((hip[1] + foot[1]) / 2 + bend), round((hip[2] + foot[2]) / 2) }
end

local function draw_leg(image, hip, foot, color)
  local ankle = { foot[1], foot[2] - 2 }
  local k = knee(hip, foot)
  line(image, hip, k, 2, color)
  line(image, k, ankle, 2, color)
  rect(image, foot[1] - 1, foot[2] - 1, foot[1] + 2, foot[2], color)
  outline(image, LINE)
end

-- Bronze greave on the shin, leather sandal with its straps.
local function draw_leg_detail(image, hip, foot)
  local k = knee(hip, foot)
  line(image, { k[1], k[2] + 1 }, { foot[1], foot[2] - 3 }, 2, { BRONZE.hi, BRONZE.mid })
  rect(image, foot[1] - 1, foot[2], foot[1] + 2, foot[2], LEATHER.dark)
  put(image, foot[1] + 1, foot[2] - 1, LEATHER.mid)
  rect(image, foot[1], foot[2] - 2, foot[1] + 1, foot[2] - 2, LEATHER.mid)
end

local function draw_arm(image, shoulder, hand, color)
  local e = elbow(shoulder, hand)
  line(image, shoulder, e, 2, color)
  line(image, e, hand, 2, color)
  rect(image, hand[1], hand[2], hand[1] + 1, hand[2] + 1, LIGHT)
  outline(image, LINE)
end

-- The manica: padded bands from the shoulder to the wrist.
local function draw_manica(image, shoulder, hand)
  local e = elbow(shoulder, hand)
  local sign = function(v) return v > 0 and 1 or (v < 0 and -1 or 0) end
  local wrist = { hand[1] - sign(hand[1] - e[1]), hand[2] - sign(hand[2] - e[2]) }
  local bands = { MANICA.cloth, MANICA.band }
  line(image, shoulder, e, 2, bands)
  line(image, e, wrist, 2, bands)
end

-- A short sword in the hand at `hand`, pointing at `angle`.
local function draw_sword(image, hand, angle)
  local a = math.rad(angle)
  local dx, dy = math.cos(a), math.sin(a)
  local at = function(d) return { round(hand[1] + 0.5 + dx * d), round(hand[2] + 0.5 + dy * d) } end
  line(image, at(-2), at(0), 1, LEATHER.dark)
  local guard = at(1)
  line(image, { round(guard[1] - dy), round(guard[2] + dx) },
    { round(guard[1] + dy), round(guard[2] - dx) }, 1, BRONZE.mid)
  line(image, at(2), at(7), 1, { STEEL.hi, STEEL.mid })
  outline(image, STEEL.line)
end

local function draw_shield(image, hand)
  local cx, cy = hand[1] + 2, hand[2]
  rect(image, cx - 2, cy - 5, cx + 3, cy + 5, BRONZE.mid)
  rect(image, cx - 1, cy - 4, cx + 2, cy + 4, WOOD.mid)
  rect(image, cx - 1, cy - 4, cx - 1, cy + 4, WOOD.dark)
  rect(image, cx, cy - 1, cx + 1, cy, BRONZE.hi)
  outline(image, BRONZE.line)
end

local function draw_torso(image, p)
  local x, y = 12 + p.bx, 13 + p.by
  rect(image, x + 1, y - 1, x + 7, y - 1, MID)
  rect(image, x, y, x + 8, y + 7, MID)
  rect(image, x + 3, y - 2, x + 5, y - 2, SHADE)
  rect(image, x, y, x + 1, y + 7, SHADE)
  rect(image, x + 5, y + 1, x + 7, y + 2, LIGHT)
  for _, abs in ipairs({ { 4, 4 }, { 6, 4 }, { 4, 6 }, { 6, 6 } }) do
    put(image, x + abs[1], y + abs[2], SHADE)
  end
  outline(image, LINE)
end

-- Scars across the chest, the belt and its buckle.
local function draw_torso_detail(image, p)
  local x, y = 12 + p.bx, 13 + p.by
  line(image, { x + 1, y + 2 }, { x + 3, y + 4 }, 1, SCAR)
  line(image, { x + 6, y + 3 }, { x + 7, y + 5 }, 1, SCAR)
  rect(image, x, y + 7, x + 8, y + 8, LEATHER.mid)
  rect(image, x, y + 8, x + 8, y + 8, LEATHER.dark)
  rect(image, x + 6, y + 7, x + 7, y + 8, BRONZE.hi)
end

local function draw_loincloth(image, p)
  local y = 22 + p.by
  rect(image, 12, y, 20, y + 1, MID)
  rect(image, 15, y + 2, 18, y + 3, MID)
  for _, x in ipairs({ 14, 17, 19 }) do
    rect(image, x, y, x, y + 1, SHADE)
  end
  rect(image, 16, y + 2, 16, y + 3, SHADE)
  outline(image, LINE)
end

-- The face and neck under the helmet, as in the first idle frame.
local function draw_head(image)
  disk(image, 17, 7, 4, MID)
  rect(image, 15, 10, 17, 12, SHADE)
end

-- The bronze helmet: dome, brim, and the visor's small holes.
local function draw_helmet(image)
  for y = 0, FRAME - 1 do
    for x = 0, FRAME - 1 do
      local dx, dy = x + 0.5 - 16, y + 0.5 - 7
      if dx * dx + dy * dy <= 4.6 * 4.6 then
        local color = BRONZE.mid
        if dx < -1 and dy < -1 then
          color = BRONZE.hi
        elseif dy > 2 or dx < -3 then
          color = BRONZE.dark
        end
        image:drawPixel(x, y, color)
      end
    end
  end
  rect(image, 18, 5, 21, 9, BRONZE.mid)
  for _, hole in ipairs({ { 19, 6 }, { 21, 6 }, { 20, 7 }, { 19, 8 }, { 21, 8 } }) do
    put(image, hole[1], hole[2], HOLE)
  end
  rect(image, 11, 10, 22, 10, BRONZE.dark)
  outline(image, BRONZE.line)
end

-- The crest's horsehair along the top of the helmet.
local function draw_crest(image)
  rect(image, 13, 1, 19, 1, MID)
  rect(image, 12, 2, 20, 2, MID)
  rect(image, 11, 3, 20, 3, SHADE)
  for x = 12, 20, 2 do
    rect(image, x, 1, x, 2, LIGHT)
  end
  outline(image, LINE)
end

-- What each layer holds, given a frame's joints; `head` layers on the first frame only.
local DRAW = {
  leg_back = function(i, p) draw_leg(i, p.hip_back, p.foot_back, SHADE) end,
  leg_back_detail = function(i, p) draw_leg_detail(i, p.hip_back, p.foot_back) end,
  arm_back = function(i, p) draw_arm(i, p.shoulder_back, p.hand_back, SHADE) end,
  weapon_back = function(i, p) draw_sword(i, p.hand_back, BACK_SWORD) end,
  body = draw_torso,
  body_detail = draw_torso_detail,
  loincloth = draw_loincloth,
  leg_front = function(i, p) draw_leg(i, p.hip_front, p.foot_front, MID) end,
  leg_front_detail = function(i, p) draw_leg_detail(i, p.hip_front, p.foot_front) end,
  shield = function(i, p) draw_shield(i, p.hand_back) end,
  weapon = function(i, p) draw_sword(i, p.hand_front, p.sword) end,
  arm_front = function(i, p) draw_arm(i, p.shoulder_front, p.hand_front, MID) end,
  arm_front_detail = function(i, p) draw_manica(i, p.shoulder_front, p.hand_front) end,
}
local DRAW_HEAD = {
  head = draw_head,
  head_detail = draw_helmet,
  crest = draw_crest,
}

-- `image` turned by `angle` degrees clockwise around the neck, then moved by (dx, dy).
local function turn(image, angle, dx, dy)
  if angle == 0 and dx == 0 and dy == 0 then
    return image
  end
  local out = Image(FRAME, FRAME, ColorMode.RGB)
  local a = math.rad(-angle)
  local c, s = math.cos(a), math.sin(a)
  for y = 0, FRAME - 1 do
    for x = 0, FRAME - 1 do
      local qx, qy = x + 0.5 - NECK[1] - dx, y + 0.5 - NECK[2] - dy
      local sx = math.floor(qx * c - qy * s + NECK[1])
      local sy = math.floor(qx * s + qy * c + NECK[2])
      if is_drawn(image, sx, sy) then
        out:drawPixel(x, y, image:getPixel(sx, sy))
      end
    end
  end
  return out
end

local function find_layer(sprite, name)
  for _, layer in ipairs(sprite.layers) do
    if layer.name == name then
      return layer
    end
  end
  return nil
end

local function clear(sprite, layer)
  for _, cel in ipairs(layer.cels) do
    sprite:deleteCel(cel)
  end
end

local function draw(sprite)
  for name, paint in pairs(DRAW) do
    local layer = find_layer(sprite, name)
    clear(sprite, layer)
    for number, frame in ipairs(sprite.frames) do
      local image = Image(FRAME, FRAME, ColorMode.RGB)
      paint(image, pose(number))
      if number > #POSES then
        local move = HEAD_FRAMES[number]
        image = turn(image, move[3], move[1], move[2])
      end
      if not image:isEmpty() then
        sprite:newCel(layer, frame, image, Point(0, 0))
      end
    end
  end
  for name, paint in pairs(DRAW_HEAD) do
    local layer = find_layer(sprite, name)
    clear(sprite, layer)
    local image = Image(FRAME, FRAME, ColorMode.RGB)
    paint(image)
    sprite:newCel(layer, sprite.frames[1], image, Point(0, 0))
  end
end

local function main()
  local sprite = app.activeSprite
  local function fail(message)
    if app.isUIAvailable then
      app.alert { title = "Draw gladiator", text = message }
    else
      print("draw_gladiator: " .. message)
    end
  end
  if sprite == nil or sprite.width ~= FRAME or sprite.height ~= FRAME
      or #sprite.frames ~= #HEAD_FRAMES or sprite.colorMode ~= ColorMode.RGB then
    return fail("Open art/fighters/gladiateur.aseprite, made by fighter_template.lua.")
  end
  local missing, drawn = {}, false
  for _, names in ipairs({ DRAW, DRAW_HEAD }) do
    for name in pairs(names) do
      local layer = find_layer(sprite, name)
      if layer == nil then
        table.insert(missing, name)
      elseif #layer.cels > 0 then
        drawn = true
      end
    end
  end
  if #missing > 0 then
    table.sort(missing)
    return fail("Not a gladiator source, no layer " .. table.concat(missing, ", ") ..
      ": run fighter_template.lua with the look gladiateur.")
  end
  if drawn and app.isUIAvailable then
    local choice = app.alert {
      title = "Draw gladiator",
      text = "Replace what the gladiator's layers hold?",
      buttons = { "Replace", "Cancel" },
    }
    if choice ~= 1 then
      return
    end
  end
  app.transaction("Draw gladiator", function() draw(sprite) end)
  app.refresh()
end

main()
