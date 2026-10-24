-- Creates the Aseprite source of a look, art/fighters/<look>.aseprite, laid out as
-- docs/Formats_graphiques.md says: 32 x 32 px images, 16 frames, one tag per
-- animation with the game's durations, the look's layers in drawing order
-- (fighter_looks.lua), each with its `_detail` layer above it. The layers start from
-- the drawings already in assets/fighters/ when there are some.
--
-- In Aseprite: File > Scripts > Open Scripts Folder, put this script and
-- fighter_looks.lua there, then File > Scripts > Rescan Scripts Folder and run it from
-- File > Scripts.
-- Without the interface:
--   Aseprite.exe -b --script-param look=gladiateur
--     --script art/scripts/fighter_template.lua
-- (other params: hair=<shape>, root=<repository folder>, overwrite=yes).

local FRAME = 32
local COLUMNS = 4
local FRAMES_PER_TAG = 4
-- The fighter is 1 m wide in combat, 20 px.
local BODY_RADIUS = 10

local TAGS = {
  { name = "idle", duration = 0.25, loops = true },
  { name = "move", duration = 0.1, loops = true },
  { name = "attack", duration = 0.06, loops = false },
  { name = "death", duration = 0.15, loops = false },
}

local function fail(message)
  if app.isUIAvailable then
    app.alert { title = "Fighter template", text = message }
  else
    print("fighter_template: " .. message)
  end
end

local function is_repository(root)
  return root ~= "" and app.fs.isDirectory(app.fs.joinPath(root, "assets", "fighters"))
end

-- The folder of this script, or "" if it is unknown.
local function script_folder()
  local source = debug.getinfo(1, "S").source
  if source:sub(1, 1) ~= "@" then
    return ""
  end
  return app.fs.filePath(source:sub(2))
end

-- Where the repository given last time is kept, next to this script: a copy of the
-- script in Aseprite's scripts folder cannot find the repository from where it is.
local function remembered_path()
  local folder = script_folder()
  return folder ~= "" and app.fs.joinPath(folder, "lanista_root.txt") or nil
end

local function remember(root)
  local path = remembered_path()
  local file = path and io.open(path, "w")
  if file then
    file:write(root)
    file:close()
  end
end

-- The repository: two folders above this script if it was run from art/scripts,
-- otherwise the one given last time.
local function default_root()
  local folder = script_folder()
  local above = folder ~= "" and app.fs.filePath(app.fs.filePath(folder)) or ""
  if is_repository(above) then
    return above
  end
  local path = remembered_path()
  local file = path and io.open(path, "r")
  if file then
    local root = file:read("l") or ""
    file:close()
    return root
  end
  return ""
end

local function load_looks()
  local path = app.fs.joinPath(script_folder(), "fighter_looks.lua")
  if not app.fs.isFile(path) then
    return nil
  end
  return dofile(path)
end

local function sorted_keys(map)
  local keys = {}
  for key in pairs(map) do
    table.insert(keys, key)
  end
  table.sort(keys)
  return keys
end

local function ask(looks)
  local params = {
    root = app.params.root or default_root(),
    look = app.params.look or "average",
    hair = app.params.hair or "short",
    overwrite = app.params.overwrite == "yes",
  }
  if not app.isUIAvailable then
    return params
  end
  -- Asked again until the repository is right, or cancelled.
  while true do
    local dialog = Dialog("Fighter template")
    dialog:entry { id = "root", label = "Repository", text = params.root }
    dialog:combobox { id = "look", label = "Look", option = params.look,
      options = sorted_keys(looks) }
    dialog:entry { id = "hair", label = "Hair shape", text = params.hair }
    dialog:button { id = "ok", text = "Create", focus = true }
    dialog:button { id = "cancel", text = "Cancel" }
    dialog:show()
    local data = dialog.data
    if not data.ok then
      return nil
    end
    params.root, params.look, params.hair = data.root, data.look, data.hair
    if is_repository(params.root) then
      remember(params.root)
      return params
    end
    fail("No assets/fighters folder in \"" .. params.root ..
      "\": give the repository folder, such as C:\\Dev\\Lanista.")
  end
end

-- assets/fighters/<layer>/<variant><suffix>.png, or nil if there is none.
local function load_sheet(params, layer, suffix)
  local variant = layer.name == "hair" and params.hair or params.look
  local path = app.fs.joinPath(params.root, "assets", "fighters", layer.name,
    variant .. suffix .. ".png")
  if not app.fs.isFile(path) then
    return nil
  end
  return Image { fromFile = path }
end

-- Image `index` (from 0) of a 4-column spritesheet.
local function cut(sheet, index)
  local image = Image(FRAME, FRAME, ColorMode.RGB)
  local x = (index % COLUMNS) * FRAME
  local y = (index // COLUMNS) * FRAME
  image:drawImage(sheet, Point(-x, -y))
  return image
end

local function fill(sprite, layer, sheet, head)
  if sheet == nil then
    return
  end
  local count = head and 1 or #sprite.frames
  for index = 0, count - 1 do
    local image = cut(sheet, index)
    if not image:isEmpty() then
      sprite:newCel(layer, sprite.frames[index + 1], image, Point(0, 0))
    end
  end
end

-- The center of the image, the fighter's position, and its 1 m wide footprint.
local function guide_image()
  local image = Image(FRAME, FRAME, ColorMode.RGB)
  local mark = app.pixelColor.rgba(255, 0, 255, 255)
  local center = FRAME / 2
  for y = 0, FRAME - 1 do
    for x = 0, FRAME - 1 do
      local dx, dy = x + 0.5 - center, y + 0.5 - center
      if math.abs(math.sqrt(dx * dx + dy * dy) - BODY_RADIUS) < 0.5 then
        image:drawPixel(x, y, mark)
      end
    end
  end
  for y = center - 1, center do
    for x = center - 1, center do
      image:drawPixel(x, y, mark)
    end
  end
  return image
end

local function build_sprite(params, layers)
  local sprite = Sprite(FRAME, FRAME, ColorMode.RGB)
  local palette = app.fs.joinPath(params.root, "art", "palette.gpl")
  if app.fs.isFile(palette) then
    sprite:setPalette(Palette { fromFile = palette })
  end

  for number = 2, #TAGS * FRAMES_PER_TAG do
    sprite:newEmptyFrame(number)
  end
  for index, animation in ipairs(TAGS) do
    local first = (index - 1) * FRAMES_PER_TAG + 1
    local tag = sprite:newTag(first, first + FRAMES_PER_TAG - 1)
    tag.name = animation.name
    tag.aniDir = AniDir.FORWARD
    tag.repeats = animation.loops and 0 or 1
    for number = first, first + FRAMES_PER_TAG - 1 do
      sprite.frames[number].duration = animation.duration
    end
  end

  for index, rule in ipairs(layers) do
    local layer = index == 1 and sprite.layers[1] or sprite:newLayer()
    layer.name = rule.name
    -- export_fighter.lua exports the hair under this shape.
    if rule.name == "hair" then
      layer.data = params.hair
    end
    fill(sprite, layer, load_sheet(params, rule, ""), rule.head)
    local detail = sprite:newLayer()
    detail.name = rule.name .. "_detail"
    fill(sprite, detail, load_sheet(params, rule, "_detail"), rule.head)
  end

  -- Not exported: the export skips layers whose name starts with `_`.
  local guide = sprite:newLayer()
  guide.name = "_guide"
  guide.opacity = 96
  local mark = guide_image()
  for _, frame in ipairs(sprite.frames) do
    sprite:newCel(guide, frame, mark, Point(0, 0))
  end
  guide.isEditable = false
  return sprite
end

local function main()
  local looks = load_looks()
  if looks == nil then
    return fail("fighter_looks.lua must be next to this script.")
  end
  local params = ask(looks)
  if params == nil then
    return
  end
  if not is_repository(params.root) then
    return fail("No assets/fighters folder in \"" .. params.root ..
      "\": give the repository folder.")
  end
  local layers = looks[params.look]
  if layers == nil then
    return fail("No look \"" .. params.look .. "\" in fighter_looks.lua.")
  end
  local folder = app.fs.joinPath(params.root, "art", "fighters")
  local path = app.fs.joinPath(folder, params.look .. ".aseprite")
  if app.fs.isFile(path) and not params.overwrite then
    if not app.isUIAvailable then
      return fail(path .. " exists; pass overwrite=yes to replace it.")
    end
    local choice = app.alert {
      title = "Fighter template",
      text = path .. " exists.",
      buttons = { "Replace", "Cancel" },
    }
    if choice ~= 1 then
      return
    end
  end
  app.fs.makeAllDirectories(folder)
  local sprite = build_sprite(params, layers)
  sprite:saveAs(path)
  if not app.isUIAvailable then
    print("fighter_template: wrote " .. path)
  end
end

main()
