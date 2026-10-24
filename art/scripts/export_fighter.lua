-- Exports the open look source (made by fighter_template.lua) to the game's files, as
-- docs/Formats_graphiques.md says: each layer of the look (fighter_looks.lua) to
-- assets/fighters/<layer>/<look>.png (a 128 x 128 px spritesheet, 4 columns),
-- `<layer>_detail` layers to <look>_detail.png, head layers to one 32 x 32 image from
-- the first frame, the hair under its shape (hair/<shape>.png). Layers whose name
-- starts with `_` are not exported. A detail layer left empty removes its old file, so
-- the game no longer draws it.
--
-- In Aseprite: open art/fighters/<look>.aseprite, then run it from File > Scripts
-- (fighter_looks.lua must be next to it).
-- Without the interface:
--   Aseprite.exe -b art/fighters/gladiateur.aseprite
--     --script art/scripts/export_fighter.lua
-- (other params: look=<look>, hair=<shape>, root=<repository folder>).

local FRAME = 32
local COLUMNS = 4
local FRAMES = 16

local function report(message)
  if app.isUIAvailable then
    app.alert { title = "Export fighter", text = message }
  else
    print("export_fighter: " .. message)
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

-- Shared with fighter_template.lua: the repository given last time, next to the script.
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

-- The hair shape fighter_template.lua stored on the hair layer, if any.
local function stored_hair(sprite)
  for _, layer in ipairs(sprite.layers) do
    if layer.name == "hair" and layer.data ~= "" then
      return layer.data
    end
  end
  return "short"
end

local function ask(sprite)
  local params = {
    root = app.params.root or default_root(),
    look = app.params.look or app.fs.fileTitle(sprite.filename),
    hair = app.params.hair or stored_hair(sprite),
  }
  if not app.isUIAvailable then
    return params
  end
  -- Asked again until the repository is right, or cancelled.
  while true do
    local dialog = Dialog("Export fighter")
    dialog:entry { id = "root", label = "Repository", text = params.root }
    dialog:entry { id = "look", label = "Look", text = params.look }
    dialog:entry { id = "hair", label = "Hair shape", text = params.hair }
    dialog:button { id = "ok", text = "Export", focus = true }
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
    report("No assets/fighters folder in \"" .. params.root ..
      "\": give the repository folder, such as C:\\Dev\\Lanista.")
  end
end

-- What `layer` holds on frame `number`, cut to the 32 x 32 image.
local function frame_image(layer, number)
  local image = Image(FRAME, FRAME, ColorMode.RGB)
  local cel = layer:cel(number)
  if cel then
    image:drawImage(cel.image, cel.position)
  end
  return image
end

-- The layer's 4-column spritesheet, or its first frame alone; nil if it is empty.
local function layer_image(layer, head)
  if head then
    local image = frame_image(layer, 1)
    return not image:isEmpty() and image or nil
  end
  local sheet = Image(FRAME * COLUMNS, FRAME * (FRAMES // COLUMNS), ColorMode.RGB)
  for index = 0, FRAMES - 1 do
    local x = (index % COLUMNS) * FRAME
    local y = (index // COLUMNS) * FRAME
    sheet:drawImage(frame_image(layer, index + 1), Point(x, y))
  end
  return not sheet:isEmpty() and sheet or nil
end

-- `<layer>` or `<layer>_detail` of the look: its rule and whether it is the detail.
local function split(rules, name)
  for _, rule in ipairs(rules) do
    if name == rule.name then
      return rule, false
    elseif name == rule.name .. "_detail" then
      return rule, true
    end
  end
  return nil
end

local function export(sprite, params, rules)
  local written, removed, skipped = {}, {}, {}
  for _, layer in ipairs(sprite.layers) do
    local name = layer.name
    if name:sub(1, 1) ~= "_" then
      local rule, detail = split(rules, name)
      if rule == nil then
        table.insert(skipped, name .. " (not a layer of the look)")
      elseif layer.isGroup then
        table.insert(skipped, name .. " (group)")
      else
        local variant = rule.name == "hair" and params.hair or params.look
        local folder = app.fs.joinPath(params.root, "assets", "fighters", rule.name)
        local file = variant .. (detail and "_detail" or "") .. ".png"
        local path = app.fs.joinPath(folder, file)
        local image = layer_image(layer, rule.head)
        if image then
          app.fs.makeAllDirectories(folder)
          image:saveAs(path)
          table.insert(written, rule.name .. "/" .. file)
        elseif detail then
          if app.fs.isFile(path) then
            os.remove(path)
            table.insert(removed, rule.name .. "/" .. file)
          end
        else
          table.insert(skipped, name .. " (empty, the game needs it)")
        end
      end
    end
  end
  local lines = { "Written in assets/fighters: " .. table.concat(written, ", ") }
  if #removed > 0 then
    table.insert(lines, "Removed (empty detail): " .. table.concat(removed, ", "))
  end
  if #skipped > 0 then
    table.insert(lines, "Not exported: " .. table.concat(skipped, ", "))
  end
  report(table.concat(lines, "\n"))
end

local function main()
  local sprite = app.activeSprite
  if sprite == nil then
    return report("Open a look source first (art/fighters/<look>.aseprite).")
  end
  if sprite.width ~= FRAME or sprite.height ~= FRAME or #sprite.frames ~= FRAMES
      or sprite.colorMode ~= ColorMode.RGB then
    return report("A look source is 32 x 32 px, 16 frames, RGB color mode.")
  end
  local looks = load_looks()
  if looks == nil then
    return report("fighter_looks.lua must be next to this script.")
  end
  local params = ask(sprite)
  if params == nil then
    return
  end
  if not is_repository(params.root) then
    return report("No assets/fighters folder in \"" .. params.root ..
      "\": pass root=<repository folder>.")
  end
  local rules = looks[params.look]
  if rules == nil then
    return report("No look \"" .. params.look .. "\" in fighter_looks.lua.")
  end
  if params.hair == "" then
    return report("Give the hair shape.")
  end
  export(sprite, params, rules)
end

main()
