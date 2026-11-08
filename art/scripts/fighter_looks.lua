-- The layers of each look, in drawing order, as `appearance` in data/fighter.ron lists
-- them: keep both the same. `head` layers are one 32 x 32 image that the game moves
-- with the head; the `hair` layer is saved under the hair shape, the others under the
-- look's name. `average` is the common look, a class's own look is named after its key.
-- `throws` gives a look the fifth animation, the throw of a mine or a trap.
-- Read by fighter_template.lua and export_fighter.lua; running it does nothing.
return {
  average = {
    { name = "leg_back" },
    { name = "arm_back" },
    { name = "body" },
    { name = "leg_front" },
    { name = "head", head = true },
    { name = "hair", head = true },
    { name = "arm_front" },
  },
  gladiateur = {
    { name = "leg_back" },
    { name = "arm_back" },
    { name = "weapon_back" },
    { name = "body" },
    { name = "loincloth" },
    { name = "leg_front" },
    { name = "shield" },
    { name = "head", head = true },
    { name = "crest", head = true },
    { name = "weapon" },
    { name = "arm_front" },
  },
  ["maitre-terrain"] = {
    { name = "leg_back" },
    { name = "arm_back" },
    { name = "weapon_back" },
    { name = "body" },
    { name = "cloak" },
    { name = "leg_front" },
    { name = "skirt" },
    { name = "shield" },
    { name = "head", head = true },
    { name = "hair", head = true },
    { name = "weapon" },
    { name = "arm_front" },
  },
  chasseur = {
    throws = true,
    { name = "quiver" },
    { name = "leg_back" },
    { name = "arm_back" },
    { name = "trap" },
    { name = "body" },
    { name = "mantle" },
    { name = "leg_front" },
    { name = "head", head = true },
    { name = "hood", head = true },
    { name = "bow_heavy" },
    { name = "bow_light" },
    { name = "arm_front" },
  },
}
