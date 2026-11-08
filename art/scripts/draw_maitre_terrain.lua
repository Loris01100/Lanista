-- Draws a first master of the ground on the open art/fighters/maitre-terrain.aseprite
-- (made by fighter_template.lua with the look `maitre-terrain`), a base to redraw by
-- hand: a hooded cloak in the club's colour, the hood down on the shoulders, a canvas
-- tunic, a belt with a satchel for sand and stones, leather bands on the forearms, feet
-- wrapped in bands, short daggers with runes engraved in the steel, a round wooden
-- shield or a second dagger. Its attack throws: the front arm swings from behind the
-- head to straight ahead, the dagger pointing at the target. Tinted layers are drawn in
-- light grey (the game multiplies them by the fighter's tints: skin, hair, and the
-- club's for the cloak), canvas, leather, wood and metal in their own colours, in the
-- `_detail` layers or the plain layers (body, skirt, weapon, weapon_back, shield).
--
-- The head has the common look's shape, so that its hair shapes fit it.
--
-- The 16 frames follow docs/Formats_graphiques.md (idle, move, attack, death) and the
-- head's movement in data/fighter.ron (`head_frames`): the upper body moves like the
-- head, and in death the whole body turns around the neck as the head does, so that
-- the head the game moves stays on the body. Head layers are drawn on the first frame
-- only, as the game uses that one alone.
--
-- In Aseprite: open art/fighters/maitre-terrain.aseprite, then run it from File >
-- Scripts. Its drawing replaces what the layers it draws hold (undo with Ctrl+Z).
-- Without the interface:
--   Aseprite.exe -b art/fighters/maitre-terrain.aseprite
--     --script art/scripts/draw_maitre_terrain.lua
-- (the param save=yes saves the file afterwards).

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

-- Feet (back, front), hands (back, front) before the upper body's move, the dagger's
-- angle in degrees (0 points ahead, negative up) and how far back the cape flies, for
-- frames 1 to 12; death frames take the first one.
local POSES = {
  -- idle
  { feet = { { 13, 30 }, { 19, 30 } }, hands = { { 21, 17 }, { 21, 19 } }, dagger = -20, cape = 0 },
  { feet = { { 13, 30 }, { 19, 30 } }, hands = { { 21, 17 }, { 21, 19 } }, dagger = -20, cape = 0 },
  { feet = { { 13, 30 }, { 19, 30 } }, hands = { { 21, 17 }, { 21, 19 } }, dagger = -20, cape = 0 },
  { feet = { { 13, 30 }, { 19, 30 } }, hands = { { 21, 17 }, { 21, 19 } }, dagger = -20, cape = 0 },
  -- move
  { feet = { { 11, 30 }, { 20, 30 } }, hands = { { 21, 17 }, { 20, 19 } }, dagger = -15, cape = 1 },
  { feet = { { 15, 29 }, { 17, 30 } }, hands = { { 21, 17 }, { 21, 19 } }, dagger = -20, cape = 2 },
  { feet = { { 20, 30 }, { 12, 30 } }, hands = { { 21, 18 }, { 22, 19 } }, dagger = -25, cape = 1 },
  { feet = { { 17, 30 }, { 15, 29 } }, hands = { { 21, 17 }, { 21, 19 } }, dagger = -20, cape = 2 },
  -- attack, a throw: arm behind the head, over it, straight ahead (the impact), back
  { feet = { { 13, 30 }, { 19, 30 } }, hands = { { 21, 18 }, { 13, 10 } }, dagger = -135, cape = 0 },
  { feet = { { 13, 30 }, { 20, 30 } }, hands = { { 21, 18 }, { 19, 9 } }, dagger = -70, cape = 1 },
  { feet = { { 12, 30 }, { 22, 30 } }, hands = { { 20, 18 }, { 26, 14 } }, dagger = -5, cape = 2 },
  { feet = { { 13, 30 }, { 20, 30 } }, hands = { { 21, 18 }, { 24, 17 } }, dagger = 10, cape = 1 },
}
-- The second dagger's angle, held low ahead.
local BACK_DAGGER = 15

local function rgb(r, g, b)
  return app.pixelColor.rgba(r, g, b, 255)
end

-- Greys of the tinted layers: the tint at full on LIGHT, darker below.
local LIGHT, MID, SHADE, DARK, LINE = rgb(240, 240, 240), rgb(208, 208, 208),
    rgb(168, 168, 168), rgb(132, 132, 132), rgb(80, 80, 80)
local CANVAS = { hi = rgb(214, 200, 166), mid = rgb(184, 166, 128), dark = rgb(142, 124, 92),
  line = rgb(78, 64, 46) }
local WRAP = { hi = rgb(190, 176, 144), mid = rgb(150, 134, 104) }
local LEATHER = { hi = rgb(176, 118, 70), mid = rgb(140, 88, 50), dark = rgb(100, 60, 32),
  line = rgb(60, 36, 20) }
local STEEL = { hi = rgb(214, 222, 234), mid = rgb(150, 162, 182), rune = rgb(64, 74, 96),
  line = rgb(40, 44, 58) }
local HANDLE = { mid = rgb(184, 118, 64), dark = rgb(128, 78, 40) }
local SHIELD = { wood = rgb(146, 54, 40), plank = rgb(108, 36, 28), rim = rgb(70, 70, 78),
  boss = rgb(156, 158, 168), shine = rgb(212, 214, 222), rivet = rgb(206, 170, 84),
  line = rgb(36, 30, 32) }
local EYE = rgb(36, 26, 20)

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
    dagger = base.dagger,
    cape = base.cape,
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

-- Bands wrapped around the ankle and the arch, the toes bare.
local function draw_wraps(image, foot)
  rect(image, foot[1], foot[2] - 2, foot[1] + 1, foot[2] - 2, WRAP.hi)
  rect(image, foot[1] - 1, foot[2], foot[1] + 1, foot[2], WRAP.mid)
  put(image, foot[1], foot[2] - 1, WRAP.mid)
end

local function draw_arm(image, shoulder, hand, color)
  local e = elbow(shoulder, hand)
  line(image, shoulder, e, 2, color)
  line(image, e, hand, 2, color)
  rect(image, hand[1], hand[2], hand[1] + 1, hand[2] + 1, LIGHT)
  outline(image, LINE)
end

-- Leather bands from the elbow to the wrist.
local function draw_bracer(image, shoulder, hand)
  local e = elbow(shoulder, hand)
  local sign = function(v) return v > 0 and 1 or (v < 0 and -1 or 0) end
  local wrist = { hand[1] - sign(hand[1] - e[1]), hand[2] - sign(hand[2] - e[2]) }
  local from = { round((e[1] * 2 + wrist[1]) / 3), round((e[2] * 2 + wrist[2]) / 3) }
  line(image, from, wrist, 2, { LEATHER.mid, LEATHER.dark })
end

-- A short dagger in the hand at `hand`, pointing at `angle`: wooden handle, small
-- guard, a blade with runes engraved in the steel.
local function draw_dagger(image, hand, angle)
  local a = math.rad(angle)
  local dx, dy = math.cos(a), math.sin(a)
  local at = function(d) return { round(hand[1] + 0.5 + dx * d), round(hand[2] + 0.5 + dy * d) } end
  line(image, at(-2), at(0), 1, { HANDLE.dark, HANDLE.mid })
  local guard = at(1)
  line(image, { round(guard[1] - dy), round(guard[2] + dx) },
    { round(guard[1] + dy), round(guard[2] - dx) }, 1, STEEL.mid)
  line(image, at(2), at(5), 1, { STEEL.hi, STEEL.rune, STEEL.mid, STEEL.rune })
  outline(image, STEEL.line)
end

-- A round shield of red planks, seen from the front: iron rim, boss and brass rivets.
local function draw_shield(image, hand)
  local cx, cy = hand[1] + 2.5, hand[2] + 0.5
  for y = 0, FRAME - 1 do
    for x = 0, FRAME - 1 do
      local dx, dy = x + 0.5 - cx, y + 0.5 - cy
      local d = dx * dx + dy * dy
      if d <= 5.2 * 5.2 then
        local color = SHIELD.rim
        if d <= 1.6 * 1.6 then
          color = (dx < 0 and dy < 0) and SHIELD.shine or SHIELD.boss
        elseif d <= 4.2 * 4.2 then
          color = ((y - round(cy)) % 3 == 0) and SHIELD.plank or SHIELD.wood
        end
        image:drawPixel(x, y, color)
      end
    end
  end
  local rx, ry = round(cx - 0.5), round(cy - 0.5)
  for _, rivet in ipairs({ { 0, -5 }, { 0, 5 }, { -5, 0 }, { 5, 0 } }) do
    put(image, rx + rivet[1], ry + rivet[2], SHIELD.rivet)
  end
  outline(image, SHIELD.line)
end

-- The canvas tunic's top and its belt.
local function draw_torso(image, p)
  local x, y = 12 + p.bx, 13 + p.by
  rect(image, x + 1, y - 1, x + 7, y - 1, CANVAS.mid)
  rect(image, x, y, x + 8, y + 7, CANVAS.mid)
  rect(image, x, y, x + 1, y + 7, CANVAS.dark)
  rect(image, x + 5, y + 1, x + 7, y + 3, CANVAS.hi)
  rect(image, x + 4, y, x + 4, y + 2, CANVAS.dark)
  rect(image, x, y + 7, x + 8, y + 8, LEATHER.mid)
  rect(image, x, y + 8, x + 8, y + 8, LEATHER.dark)
  put(image, x + 6, y + 7, LEATHER.hi)
  outline(image, CANVAS.line)
end

-- The cloak in the club's colour: the hood down behind the neck, the mantle on the
-- shoulders, the cape down the back to the knees, flying back as he moves.
local function draw_cloak(image, p)
  local x, y = 12 + p.bx, 13 + p.by
  -- The hood, gathered behind the neck.
  rect(image, x - 1, y - 3, x + 2, y - 1, MID)
  rect(image, x - 2, y - 2, x - 2, y + 1, SHADE)
  rect(image, x, y - 3, x + 1, y - 3, LIGHT)
  -- The mantle on the shoulders.
  rect(image, x - 1, y - 1, x + 7, y + 1, MID)
  rect(image, x + 1, y - 1, x + 6, y - 1, LIGHT)
  rect(image, x + 6, y + 1, x + 7, y + 2, SHADE)
  -- The cape down the back, wider at the hem.
  local hem = 25 - p.cape
  for row = y + 1, hem do
    local back = x - 2 - math.floor((row - y) * p.cape / 10) - (row > hem - 3 and 1 or 0)
    rect(image, back, row, x, row, row % 4 == 0 and DARK or SHADE)
  end
  outline(image, LINE)
end

-- The canvas tunic below the belt, and the satchel of sand and stones at the hip.
local function draw_skirt(image, p)
  local y = 22 + p.by
  rect(image, 12, y, 20, y + 1, CANVAS.mid)
  rect(image, 12, y + 2, 21, y + 3, CANVAS.mid)
  rect(image, 12, y, 13, y + 3, CANVAS.dark)
  for _, x in ipairs({ 15, 18 }) do
    rect(image, x, y + 1, x, y + 3, CANVAS.dark)
  end
  rect(image, 12, y + 3, 21, y + 3, CANVAS.dark)
  outline(image, CANVAS.line)
  -- The satchel hangs from the belt, its flap shut.
  local sx = 14 + p.bx
  rect(image, sx, y - 1, sx + 2, y + 1, LEATHER.mid)
  rect(image, sx, y - 1, sx + 2, y - 1, LEATHER.dark)
  put(image, sx + 1, y, LEATHER.hi)
  for row = y - 1, y + 1 do
    put(image, sx - 1, row, LEATHER.line)
    put(image, sx + 3, row, LEATHER.line)
  end
  rect(image, sx, y + 2, sx + 2, y + 2, LEATHER.line)
end

-- The face and neck, with the common look's head shape, as in the first idle frame.
local function draw_head(image)
  local rows = { { 3, 15, 19 }, { 4, 14, 20 }, { 5, 13, 21 }, { 6, 12, 21 }, { 7, 12, 21 },
    { 8, 12, 22 }, { 9, 12, 21 }, { 10, 13, 20 }, { 11, 14, 19 }, { 12, 15, 18 } }
  for _, row in ipairs(rows) do
    rect(image, row[2], row[1], row[3], row[1], MID)
  end
  rect(image, 13, 9, 15, 11, SHADE)
  rect(image, 15, 12, 17, 13, SHADE)
  outline(image, LINE)
end

-- The eye, the brow and the mouth, looking ahead.
local function draw_face(image)
  put(image, 20, 7, EYE)
  rect(image, 19, 6, 20, 6, rgb(92, 64, 44))
  put(image, 21, 10, rgb(150, 92, 78))
end

-- What each layer holds, given a frame's joints; `head` layers on the first frame only.
local DRAW = {
  leg_back = function(i, p) draw_leg(i, p.hip_back, p.foot_back, SHADE) end,
  leg_back_detail = function(i, p) draw_wraps(i, p.foot_back) end,
  arm_back = function(i, p) draw_arm(i, p.shoulder_back, p.hand_back, SHADE) end,
  arm_back_detail = function(i, p) draw_bracer(i, p.shoulder_back, p.hand_back) end,
  weapon_back = function(i, p) draw_dagger(i, p.hand_back, BACK_DAGGER) end,
  body = draw_torso,
  cloak = draw_cloak,
  leg_front = function(i, p) draw_leg(i, p.hip_front, p.foot_front, MID) end,
  leg_front_detail = function(i, p) draw_wraps(i, p.foot_front) end,
  skirt = draw_skirt,
  shield = function(i, p) draw_shield(i, p.hand_back) end,
  weapon = function(i, p) draw_dagger(i, p.hand_front, p.dagger) end,
  arm_front = function(i, p) draw_arm(i, p.shoulder_front, p.hand_front, MID) end,
  arm_front_detail = function(i, p) draw_bracer(i, p.shoulder_front, p.hand_front) end,
}
local DRAW_HEAD = {
  head = draw_head,
  head_detail = draw_face,
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
      app.alert { title = "Draw master of the ground", text = message }
    else
      print("draw_maitre_terrain: " .. message)
    end
  end
  if sprite == nil or sprite.width ~= FRAME or sprite.height ~= FRAME
      or #sprite.frames ~= #HEAD_FRAMES or sprite.colorMode ~= ColorMode.RGB then
    return fail("Open art/fighters/maitre-terrain.aseprite, made by fighter_template.lua.")
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
    return fail("Not a master of the ground source, no layer " ..
      table.concat(missing, ", ") ..
      ": run fighter_template.lua with the look maitre-terrain.")
  end
  if drawn and app.isUIAvailable then
    local choice = app.alert {
      title = "Draw master of the ground",
      text = "Replace what the master of the ground's layers hold?",
      buttons = { "Replace", "Cancel" },
    }
    if choice ~= 1 then
      return
    end
  end
  app.transaction("Draw master of the ground", function() draw(sprite) end)
  app.refresh()
  if app.params.save == "yes" then
    sprite:saveAs(sprite.filename)
  end
end

main()
