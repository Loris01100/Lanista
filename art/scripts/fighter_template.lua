-- Creates the Aseprite source of a fighter build, art/fighters/<build>.aseprite, laid
-- out as docs/Formats_graphiques.md says: 32 x 32 px images, 16 frames, one tag per
-- animation with the game's durations, one layer per part in drawing order, each with
-- its `_detail` layer above it. The layers start from the drawings already in
-- assets/fighters/ when there are some, the placeholders to begin with.
--
-- In Aseprite: File > Scripts > Open Scripts Folder, put this script there, then
-- File > Scripts > Rescan Scripts Folder and run it from File > Scripts.
-- Without the interface:
--   Aseprite.exe -b --script-param build=average --script-param hair=short
--     --script art/scripts/fighter_template.lua
-- (other params: root=<repository folder>, overwrite=yes).

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

-- Drawing order, bottom to top. The head and the hair are one 32 x 32 image each,
-- on the first frame only: the game moves and turns them to follow the body.
local PARTS = {
  { name = "leg_back" },
  { name = "arm_back" },
  { name = "body" },
  { name = "leg_front" },
  { name = "head", single = true },
  { name = "hair", single = true },
  { name = "arm_front" },
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

local function ask()
  local params = {
    root = app.params.root or default_root(),
    build = app.params.build or "average",
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
    dialog:entry { id = "build", label = "Build", text = params.build }
    dialog:entry { id = "hair", label = "Hair shape", text = params.hair }
    dialog:button { id = "ok", text = "Create", focus = true }
    dialog:button { id = "cancel", text = "Cancel" }
    dialog:show()
    local data = dialog.data
    if not data.ok then
      return nil
    end
    params.root, params.build, params.hair = data.root, data.build, data.hair
    if is_repository(params.root) then
      remember(params.root)
      return params
    end
    fail("No assets/fighters folder in \"" .. params.root ..
      "\": give the repository folder, such as C:\\Dev\\Lanista.")
  end
end

-- assets/fighters/<part>/<variant><suffix>.png, or nil if there is none.
local function load_sheet(params, part, suffix)
  local variant = part.name == "hair" and params.hair or params.build
  local path = app.fs.joinPath(params.root, "assets", "fighters", part.name,
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

local function fill(sprite, layer, sheet, single)
  if sheet == nil then
    return
  end
  local count = single and 1 or #sprite.frames
  for index = 0, count - 1 do
    local image = single and cut(sheet, 0) or cut(sheet, index)
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

local function build_sprite(params)
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

  for index, part in ipairs(PARTS) do
    local layer = index == 1 and sprite.layers[1] or sprite:newLayer()
    layer.name = part.name
    fill(sprite, layer, load_sheet(params, part, ""), part.single)
    local detail = sprite:newLayer()
    detail.name = part.name .. "_detail"
    fill(sprite, detail, load_sheet(params, part, "_detail"), part.single)
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
  local params = ask()
  if params == nil then
    return
  end
  if not is_repository(params.root) then
    return fail("No assets/fighters folder in \"" .. params.root ..
      "\": give the repository folder.")
  end
  local folder = app.fs.joinPath(params.root, "art", "fighters")
  local path = app.fs.joinPath(folder, params.build .. ".aseprite")
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
  local sprite = build_sprite(params)
  sprite:saveAs(path)
  if not app.isUIAvailable then
    print("fighter_template: wrote " .. path)
  end
end

main()
