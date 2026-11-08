-- Draws a first hunter on the open art/fighters/chasseur.aseprite (made by
-- fighter_template.lua with the look `chasseur`), a base to redraw by hand: a hood and
-- a short mantle in the club's colour, a leather jerkin and trousers, a quiver on the
-- back, a bracer on the bow arm, boots, and a long bow (`bow_heavy`) or a short one
-- with horn tips (`bow_light`), held in the front hand, the back hand on the string.
-- Its attack nocks, draws and looses an arrow; its throw (the fifth animation) swings
-- a bear trap underhand with the back hand. Tinted layers are drawn in light grey (the
-- game multiplies them by the fighter's tints: skin, and the club's for the hood and
-- the mantle), leather, wood and metal in their own colours, in the `_detail` layers
-- or the plain layers (quiver, legs, trap, body, bows).
--
-- The hood covers the head and its hair: the look has no hair layer.
--
-- The 20 frames follow docs/Formats_graphiques.md (idle, move, attack, death, throw)
-- and the head's movement in data/fighter.ron (`head_frames`): the upper body moves
-- like the head, and in death the whole body turns around the neck as the head does,
-- so that the head the game moves stays on the body. Head layers are drawn on the
-- first frame only, as the game uses that one alone.
--
-- In Aseprite: open art/fighters/chasseur.aseprite, then run it from File > Scripts.
-- Its drawing replaces what the layers it draws hold (undo with Ctrl+Z).
-- Without the interface:
--   Aseprite.exe -b art/fighters/chasseur.aseprite
--     --script art/scripts/draw_chasseur.lua
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
  { -1, 2, -5 }, { 1, 3, 10 }, { 2, 2, 15 }, { 1, 1, 5 },
}
-- The frames of death, drawn as the first one turned around the neck.
local DEATH = { first = 13, last = 16 }

-- Feet (back, front), hands (back, front) before the upper body's move, the bow's aim
-- in degrees (0 ahead, negative up), whether the string is drawn to the back hand and
-- an arrow nocked, the trap in the back hand or flying (`trap`, a point), and how far
-- back the mantle flies. Death frames take the first one.
local REST = { feet = { { 13, 30 }, { 19, 30 } }, hands = { { 15, 20 }, { 24, 19 } }, aim = 20,
  cape = 0 }
local POSES = {
  -- idle
  REST, REST, REST, REST,
  -- move
  { feet = { { 11, 30 }, { 20, 30 } }, hands = { { 14, 20 }, { 24, 19 } }, aim = 25, cape = 1 },
  { feet = { { 15, 29 }, { 17, 30 } }, hands = { { 15, 20 }, { 24, 19 } }, aim = 20, cape = 2 },
  { feet = { { 20, 30 }, { 12, 30 } }, hands = { { 17, 20 }, { 23, 19 } }, aim = 15, cape = 1 },
  { feet = { { 17, 30 }, { 15, 29 } }, hands = { { 15, 20 }, { 24, 19 } }, aim = 20, cape = 2 },
  -- attack: nock, full draw, loose (the impact), lower the bow
  { feet = { { 12, 30 }, { 20, 30 } }, hands = { { 22, 13 }, { 24, 13 } }, aim = -5, drawn = true,
    cape = 0 },
  { feet = { { 12, 30 }, { 21, 30 } }, hands = { { 18, 12 }, { 26, 12 } }, aim = -5, drawn = true,
    cape = 0 },
  { feet = { { 12, 30 }, { 21, 30 } }, hands = { { 14, 11 }, { 26, 12 } }, aim = -5, cape = 1 },
  { feet = { { 13, 30 }, { 20, 30 } }, hands = { { 16, 16 }, { 24, 15 } }, aim = 10, cape = 0 },
  -- death, from the first frame
  nil, nil, nil, nil,
  -- throw: the trap swung back low, forward, let go (the impact), the arm follows
  { feet = { { 12, 30 }, { 20, 30 } }, hands = { { 11, 22 }, { 21, 20 } }, aim = 40,
    trap = "hand", cape = 1 },
  { feet = { { 12, 30 }, { 21, 30 } }, hands = { { 16, 24 }, { 21, 20 } }, aim = 40,
    trap = "hand", cape = 2 },
  { feet = { { 12, 30 }, { 22, 30 } }, hands = { { 23, 18 }, { 21, 20 } }, aim = 40,
    trap = { 27, 17 }, cape = 2 },
  { feet = { { 13, 30 }, { 21, 30 } }, hands = { { 21, 19 }, { 21, 19 } }, aim = 30, cape = 1 },
}

local function rgb(r, g, b)
  return app.pixelColor.rgba(r, g, b, 255)
end

-- Greys of the tinted layers: the tint at full on LIGHT, darker below.
local LIGHT, MID, SHADE, DARK, LINE = rgb(240, 240, 240), rgb(208, 208, 208),
    rgb(168, 168, 168), rgb(132, 132, 132), rgb(80, 80, 80)
local LEATHER = { hi = rgb(170, 116, 72), mid = rgb(134, 86, 50), dark = rgb(98, 60, 34),
  line = rgb(56, 34, 20) }
local TROUSER = { mid = rgb(104, 82, 60), dark = rgb(78, 60, 44), line = rgb(46, 34, 24) }
local BOOT = { mid = rgb(74, 50, 34), dark = rgb(54, 36, 24), cuff = rgb(176, 160, 132) }
local BRACER = { mid = rgb(84, 56, 36), lace = rgb(196, 172, 128) }
local BUCKLE = rgb(196, 170, 96)
local FLETCH = { rgb(200, 52, 40), rgb(232, 228, 214) }
local BOWS = {
  heavy = { length = 10, depth = 2.5, hi = rgb(222, 186, 120), mid = rgb(182, 140, 82),
    tip = nil, line = rgb(70, 48, 28) },
  light = { length = 6.5, depth = 2, hi = rgb(150, 96, 56), mid = rgb(112, 68, 38),
    tip = rgb(226, 214, 186), line = rgb(52, 32, 18) },
}
local STRING = rgb(210, 204, 186)
local SHAFT = rgb(176, 136, 86)
local STEEL = { hi = rgb(210, 216, 226), mid = rgb(130, 138, 150), line = rgb(40, 44, 52) }
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

local function is_death(number)
  return number >= DEATH.first and number <= DEATH.last
end

-- The joints of frame `number`, death frames standing as the first one.
local function pose(number)
  local still = is_death(number)
  local base = POSES[still and 1 or number]
  local move = HEAD_FRAMES[still and 1 or number]
  local bx, by = round(move[1]), round(move[2])
  local hand_back = add(base.hands[1], bx, by)
  local trap = base.trap
  if trap == "hand" then
    trap = hand_back
  elseif trap then
    trap = add(trap, bx, by)
  end
  return {
    bx = bx,
    by = by,
    shoulder_back = { 14 + bx, 13 + by },
    shoulder_front = { 18 + bx, 13 + by },
    hip_back = { 15, 21 + by },
    hip_front = { 17, 21 + by },
    foot_back = base.feet[1],
    foot_front = base.feet[2],
    hand_back = hand_back,
    hand_front = add(base.hands[2], bx, by),
    aim = base.aim,
    drawn = base.drawn,
    trap = trap,
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

-- A leg in leather trousers and a boot with a fur cuff; `back` darker.
local function draw_leg(image, hip, foot, back)
  local ankle = { foot[1], foot[2] - 2 }
  local k = knee(hip, foot)
  local color = back and TROUSER.dark or TROUSER.mid
  line(image, hip, k, 2, color)
  line(image, k, ankle, 2, color)
  local boot = back and BOOT.dark or BOOT.mid
  rect(image, foot[1], foot[2] - 4, foot[1] + 1, foot[2], boot)
  rect(image, foot[1] - 1, foot[2] - 1, foot[1] + 2, foot[2], boot)
  rect(image, foot[1], foot[2] - 4, foot[1] + 1, foot[2] - 4, BOOT.cuff)
  outline(image, TROUSER.line)
end

local function draw_arm(image, shoulder, hand, color)
  local e = elbow(shoulder, hand)
  line(image, shoulder, e, 2, color)
  line(image, e, hand, 2, color)
  rect(image, hand[1], hand[2], hand[1] + 1, hand[2] + 1, LIGHT)
  outline(image, LINE)
end

-- The leather sleeve down to the wrist, the hand bare; a laced bracer on the bow arm.
local function draw_sleeve(image, shoulder, hand, bracer)
  local e = elbow(shoulder, hand)
  local sign = function(v) return v > 0 and 1 or (v < 0 and -1 or 0) end
  local wrist = { hand[1] - sign(hand[1] - e[1]), hand[2] - sign(hand[2] - e[2]) }
  line(image, shoulder, e, 2, LEATHER.mid)
  if bracer then
    line(image, e, wrist, 2, { BRACER.mid, BRACER.lace })
  else
    line(image, e, wrist, 2, LEATHER.dark)
  end
end

-- The bow in the front hand, its back towards the aim: two limbs bent around the grip,
-- the string to the back hand when drawn, an arrow on it, straight otherwise.
local function draw_bow(image, p, bow)
  local a = math.rad(p.aim)
  local dx, dy = math.cos(a), math.sin(a)
  local grip = { p.hand_front[1] + 0.5, p.hand_front[2] + 0.5 }
  local at = function(t)
    return { grip[1] - dy * t * bow.length - dx * bow.depth * t * t,
      grip[2] + dx * t * bow.length - dy * bow.depth * t * t }
  end
  local steps = math.ceil(bow.length * 3)
  for i = -steps, steps do
    local t = i / steps
    local point = at(t)
    local color = math.abs(t) < 0.15 and LEATHER.dark
        or (math.abs(t) > 0.85 and bow.tip) or (i % 3 == 0 and bow.hi or bow.mid)
    put(image, round(point[1] - 0.5), round(point[2] - 0.5), color)
  end
  outline(image, bow.line)
  local px = function(point) return { round(point[1] - 0.5), round(point[2] - 0.5) } end
  local top, bottom = px(at(-1)), px(at(1))
  if p.drawn then
    local nock = p.hand_back
    line(image, top, nock, 1, STRING)
    line(image, nock, bottom, 1, STRING)
    -- The arrow, from the string past the grip.
    local tip = px({ grip[1] + dx * 3, grip[2] + dy * 3 })
    line(image, nock, tip, 1, SHAFT)
    put(image, tip[1], tip[2], STEEL.hi)
    put(image, round(tip[1] + dx), round(tip[2] + dy), STEEL.mid)
    put(image, nock[1], nock[2] - 1, FLETCH[1])
    put(image, nock[1] + 1, nock[2] - 1, FLETCH[2])
  else
    line(image, top, bottom, 1, STRING)
  end
end

-- An iron bear trap, its jaws open and toothed.
local function draw_trap(image, p)
  if not p.trap then
    return
  end
  local x, y = p.trap[1] - 1, p.trap[2] + 1
  rect(image, x, y + 1, x + 3, y + 1, STEEL.mid)
  for i = 0, 3 do
    put(image, x + i, y, i % 2 == 0 and STEEL.hi or STEEL.mid)
  end
  put(image, x - 1, y - 1, STEEL.mid)
  put(image, x + 4, y - 1, STEEL.mid)
  put(image, x + 1, y + 2, STEEL.line)
  put(image, x + 2, y + 2, STEEL.line)
  outline(image, STEEL.line)
end

-- The quiver on the back, slanting, its arrows' fletching over the shoulder.
local function draw_quiver(image, p)
  local bottom, top = { 9 + p.bx, 21 + p.by }, { 12 + p.bx, 10 + p.by }
  line(image, bottom, top, 3, { LEATHER.dark, LEATHER.mid })
  rect(image, top[1], top[2], top[1] + 2, top[2], LEATHER.hi)
  outline(image, LEATHER.line)
  for i, shift in ipairs({ 0, 1, 2 }) do
    local x = top[1] + shift
    put(image, x, top[2] - 1, SHAFT)
    put(image, x, top[2] - 2 - (i % 2), FLETCH[i % 2 + 1])
    put(image, x, top[2] - 3 - (i % 2), FLETCH[(i + 1) % 2 + 1])
  end
end

-- The leather jerkin, its belt with a buckle and a pouch, and the quiver's strap
-- across the chest.
local function draw_torso(image, p)
  local x, y = 12 + p.bx, 13 + p.by
  rect(image, x + 1, y - 1, x + 7, y - 1, LEATHER.mid)
  rect(image, x, y, x + 8, y + 7, LEATHER.mid)
  rect(image, x, y, x + 1, y + 7, LEATHER.dark)
  rect(image, x + 5, y + 1, x + 7, y + 3, LEATHER.hi)
  -- The jerkin's skirt down over the hips.
  rect(image, x, y + 9, x + 8, y + 10, LEATHER.mid)
  rect(image, x, y + 9, x + 1, y + 10, LEATHER.dark)
  rect(image, x + 4, y + 9, x + 4, y + 10, LEATHER.dark)
  -- The strap, from the front shoulder down to the back hip.
  line(image, { x + 7, y }, { x + 1, y + 7 }, 1, LEATHER.dark)
  -- The belt.
  rect(image, x, y + 8, x + 8, y + 8, BOOT.dark)
  put(image, x + 7, y + 8, BUCKLE)
  outline(image, LEATHER.line)
  -- The pouch at the hip.
  rect(image, x + 3, y + 9, x + 5, y + 10, BOOT.mid)
  rect(image, x + 3, y + 9, x + 5, y + 9, BOOT.dark)
end

-- The short mantle in the club's colour around the shoulders, longer at the back and
-- flying back as he moves.
local function draw_mantle(image, p)
  local x, y = 12 + p.bx, 13 + p.by
  rect(image, x - 1, y - 1, x + 8, y + 1, MID)
  rect(image, x + 1, y - 1, x + 7, y - 1, LIGHT)
  rect(image, x + 2, y + 2, x + 7, y + 2, SHADE)
  local hem = y + 5 - math.floor(p.cape / 2)
  for row = y + 1, hem do
    local back = x - 1 - math.floor((row - y) * p.cape / 6)
    rect(image, back, row, x + 1, row, row == hem and DARK or SHADE)
  end
  outline(image, LINE)
end

-- The face and neck, with the common look's head shape, as in the first idle frame.
local HEAD_ROWS = { { 3, 15, 19 }, { 4, 14, 20 }, { 5, 13, 21 }, { 6, 12, 21 }, { 7, 12, 21 },
  { 8, 12, 22 }, { 9, 12, 21 }, { 10, 13, 20 }, { 11, 14, 19 }, { 12, 15, 18 } }

local function draw_head(image)
  for _, row in ipairs(HEAD_ROWS) do
    rect(image, row[2], row[1], row[3], row[1], MID)
  end
  rect(image, 15, 12, 17, 13, SHADE)
  outline(image, LINE)
end

-- The eye under the hood's shadow, the brow and the mouth, looking ahead.
local function draw_face(image)
  put(image, 20, 7, EYE)
  rect(image, 19, 6, 20, 6, rgb(92, 64, 44))
  put(image, 21, 10, rgb(150, 92, 78))
end

-- The hood over the head, a pixel over it at the top and the back, open on the face,
-- its brim over the brow and its cowl down behind the neck.
local function draw_hood(image)
  local face = function(x, y) return x >= 16 and y >= 6 end
  for _, row in ipairs(HEAD_ROWS) do
    local y = row[1]
    for x = row[2] - 1, row[3] do
      if not face(x, y) then
        put(image, x, y, x < 14 and SHADE or MID)
      end
    end
  end
  rect(image, 15, 2, 18, 2, MID)
  rect(image, 15, 3, 18, 3, LIGHT)
  -- The brim over the brow, a point ahead.
  rect(image, 16, 5, 21, 5, MID)
  put(image, 22, 5, SHADE)
  -- The opening's edge, in shadow.
  rect(image, 15, 7, 15, 11, DARK)
  -- The cowl behind the neck.
  rect(image, 12, 12, 14, 13, SHADE)
  outline(image, LINE)
end

-- What each layer holds, given a frame's joints; `head` layers on the first frame only.
local DRAW = {
  quiver = draw_quiver,
  leg_back = function(i, p) draw_leg(i, p.hip_back, p.foot_back, true) end,
  arm_back = function(i, p) draw_arm(i, p.shoulder_back, p.hand_back, SHADE) end,
  arm_back_detail = function(i, p) draw_sleeve(i, p.shoulder_back, p.hand_back, false) end,
  trap = draw_trap,
  body = draw_torso,
  mantle = draw_mantle,
  leg_front = function(i, p) draw_leg(i, p.hip_front, p.foot_front, false) end,
  bow_heavy = function(i, p) draw_bow(i, p, BOWS.heavy) end,
  bow_light = function(i, p) draw_bow(i, p, BOWS.light) end,
  arm_front = function(i, p) draw_arm(i, p.shoulder_front, p.hand_front, MID) end,
  arm_front_detail = function(i, p) draw_sleeve(i, p.shoulder_front, p.hand_front, true) end,
}
local DRAW_HEAD = {
  head = draw_head,
  head_detail = draw_face,
  hood = draw_hood,
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
      if is_death(number) then
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
      app.alert { title = "Draw hunter", text = message }
    else
      print("draw_chasseur: " .. message)
    end
  end
  if sprite == nil or sprite.width ~= FRAME or sprite.height ~= FRAME
      or #sprite.frames ~= #HEAD_FRAMES or sprite.colorMode ~= ColorMode.RGB then
    return fail("Open art/fighters/chasseur.aseprite, made by fighter_template.lua.")
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
    return fail("Not a hunter source, no layer " .. table.concat(missing, ", ") ..
      ": run fighter_template.lua with the look chasseur.")
  end
  if drawn and app.isUIAvailable then
    local choice = app.alert {
      title = "Draw hunter",
      text = "Replace what the hunter's layers hold?",
      buttons = { "Replace", "Cancel" },
    }
    if choice ~= 1 then
      return
    end
  end
  app.transaction("Draw hunter", function() draw(sprite) end)
  app.refresh()
  if app.params.save == "yes" then
    sprite:saveAs(sprite.filename)
  end
end

main()
