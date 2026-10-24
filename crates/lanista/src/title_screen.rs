//! The main menu, opened at launch: go on with the game the autosave holds, load a
//! save slot or start a new game, which opens the club screen over it; the options and
//! « Quitter ». Leaving the club comes back here.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use lanista_core::colosseum::Continent;
use lanista_core::design_data::DesignData;
use lanista_core::economy::Wealth;
use lanista_core::names::{self, ClubName};
use lanista_core::rivals;
use lanista_core::state::GameState;
use macroquad::prelude::*;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use reesch::{Assets, Audio, Bindings, Screen, Transition, egui};

use crate::Action;
use crate::club_screen::ClubScreen;
use crate::lang::tr;
use crate::options;
use crate::saves;
use crate::texts::{continent_name, culture_name, date_text, named_club, wealth_name};

const BACKGROUND: Color = Color::new(0.16, 0.13, 0.11, 1.0);
/// Width of the menu's buttons.
const BUTTON_WIDTH: f32 = 260.0;

/// What a save file holds, as the menus show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Slot {
    Empty,
    /// A game at this week, of a club of this name and wealth on this continent.
    Game(u64, ClubName, Wealth, Continent),
    Unreadable,
}

/// The pages of the main menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    /// « Continuer », « Charger une partie », « Nouvelle partie », options, « Quitter ».
    Main,
    /// The save slots, each loaded in one click.
    Load,
    /// The new game's continent, club and wealth.
    New,
}

/// A choice that loses the game the autosave holds, carried out only once the player
/// confirms it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Confirm {
    /// Loads this slot.
    Load(usize),
    /// Starts a new game of this wealth on this continent.
    NewGame(Wealth, Continent),
}

pub struct TitleScreen {
    data: Rc<DesignData>,
    /// Folder of the save files; `None` saves nothing.
    saves: Option<PathBuf>,
    bindings: Bindings<Action>,
    page: Page,
    /// What the autosave holds: the game « Continuer » goes on with.
    autosave: Slot,
    /// What each of `saves::SLOTS` holds.
    slots: [Slot; saves::SLOTS.len()],
    /// Why the last game could not be opened, for the player.
    message: Option<String>,
    /// The choice waiting for the player to confirm it.
    confirm: Option<Confirm>,
    /// Where the new game starts.
    new_continent: Continent,
    /// The new game's club: the key of its culture, one of `new_continent`'s, and its
    /// name, as the player chose or drew them.
    new_club: (String, ClubName),
    /// Whether the options are laid over the menu.
    options_open: bool,
    /// Set once a game is opened: the files are read again when the player comes back.
    stale: bool,
}

impl TitleScreen {
    pub fn new(data: Rc<DesignData>, saves: Option<PathBuf>, bindings: Bindings<Action>) -> Self {
        let mut screen = Self {
            data,
            saves,
            bindings,
            page: Page::Main,
            autosave: Slot::Empty,
            slots: [const { Slot::Empty }; saves::SLOTS.len()],
            message: None,
            confirm: None,
            new_continent: Continent::Europe,
            new_club: (String::new(), ClubName::default()),
            options_open: false,
            stale: false,
        };
        screen.read_files();
        screen.draw_new_club(None);
        screen
    }

    /// What the autosave and each slot hold.
    fn read_files(&mut self) {
        let folder = self.saves.as_deref();
        self.autosave = read_slot(folder, saves::AUTO, &self.data);
        self.slots = read_slots(folder, &self.data);
    }

    /// The club screen on the game the autosave holds; `None`, told to the player, if
    /// it cannot be read.
    pub(crate) fn resume(&mut self) -> Option<ClubScreen> {
        let state = self.read(saves::AUTO)?;
        Some(self.club(state).telling(tr!("save-resumed")))
    }

    /// The club screen on the game of the slot `slot`, which the autosave now holds.
    fn load(&mut self, slot: usize) -> Option<ClubScreen> {
        let state = self.read(saves::SLOTS[slot])?;
        let message = self.keep(&state).unwrap_or_else(|| tr!("save-loaded"));
        Some(self.club(state).telling(message))
    }

    /// The club screen on a new game of `wealth` on `continent`, with the club chosen,
    /// which the autosave now holds.
    fn new_game(&mut self, wealth: Wealth, continent: Continent) -> ClubScreen {
        let seed = new_seed();
        let club = self.new_club.clone();
        let state = GameState::start(seed, (wealth, continent), Some(club), &self.data);
        let message = self.keep(&state).unwrap_or_else(|| {
            tr!(
                "menu-new-game-started",
                club = named_club(&state.name),
                wealth = wealth_name(wealth),
                continent = continent_name(continent),
                seed = seed.to_string()
            )
        });
        self.club(state).telling(message)
    }

    /// The game in `file`; `None`, told to the player, if there is none or it cannot
    /// be read.
    fn read(&mut self, file: &str) -> Option<GameState> {
        let folder = self.saves.as_deref()?;
        match saves::read(folder, file, &self.data) {
            Ok(Some(state)) => return Some(state),
            Ok(None) => self.message = Some(tr!("save-none")),
            Err(e) => self.message = Some(tr!("save-unreadable", error = e)),
        }
        None
    }

    /// Writes `state` to the autosave, so that the next launch goes on with it and not
    /// with the game it replaced; the text to show the player when it failed.
    fn keep(&self, state: &GameState) -> Option<String> {
        let folder = self.saves.as_deref()?;
        let failed = saves::write(folder, saves::AUTO, state).err()?;
        Some(tr!("save-failed", error = failed.to_string()))
    }

    fn club(&mut self, state: GameState) -> ClubScreen {
        self.message = None;
        self.page = Page::Main;
        self.stale = true;
        ClubScreen::new(
            Rc::clone(&self.data),
            state,
            self.saves.clone(),
            self.bindings.clone(),
        )
    }

    /// Asks the player to confirm `choice` when it loses the game the autosave holds,
    /// or carries it out at once.
    fn ask(&mut self, choice: Confirm) -> Option<ClubScreen> {
        if matches!(self.autosave, Slot::Game(..)) {
            self.confirm = Some(choice);
            return None;
        }
        self.carry_out(choice)
    }

    fn carry_out(&mut self, choice: Confirm) -> Option<ClubScreen> {
        self.confirm = None;
        match choice {
            Confirm::Load(slot) => self.load(slot),
            Confirm::NewGame(wealth, continent) => Some(self.new_game(wealth, continent)),
        }
    }

    /// « Continuer », « Charger une partie », « Nouvelle partie », the options and
    /// « Quitter »; returns the game to open, and whether to quit.
    fn main_page(&mut self, ui: &mut egui::Ui) -> (Option<ClubScreen>, bool) {
        let mut opened = None;
        let size = egui::vec2(BUTTON_WIDTH, 0.0);
        let resumable = matches!(self.autosave, Slot::Game(..));
        let button = egui::Button::new(tr!("title-continue")).min_size(size);
        if ui.add_enabled(resumable, button).clicked() {
            opened = self.resume();
        }
        if resumable {
            ui.label(slot_text(&self.autosave, &self.data));
        }
        ui.add_space(6.0);
        let saved = self.slots.iter().any(|s| *s != Slot::Empty);
        let button = egui::Button::new(tr!("title-load")).min_size(size);
        if ui.add_enabled(saved, button).clicked() {
            self.page = Page::Load;
        }
        ui.add_space(6.0);
        let button = egui::Button::new(tr!("title-new-game")).min_size(size);
        if ui.add(button).clicked() {
            self.page = Page::New;
        }
        ui.add_space(6.0);
        let button = egui::Button::new(tr!("options")).min_size(size);
        if ui.add(button).clicked() {
            self.options_open = true;
        }
        ui.add_space(6.0);
        let quit = ui
            .add(egui::Button::new(tr!("title-quit")).min_size(size))
            .clicked();
        (opened, quit)
    }

    /// The save slots, each with « Charger »; returns the game to open.
    fn load_page(&mut self, ui: &mut egui::Ui) -> Option<ClubScreen> {
        ui.heading(tr!("title-load"));
        ui.add_space(6.0);
        let mut chosen = None;
        for (i, slot) in self.slots.iter().enumerate() {
            ui.horizontal(|ui| {
                let loadable = matches!(slot, Slot::Game(..));
                if ui
                    .add_enabled(loadable, egui::Button::new(tr!("menu-load")))
                    .clicked()
                {
                    chosen = Some(Confirm::Load(i));
                }
                ui.label(tr!(
                    "save-slot",
                    slot = i + 1,
                    game = slot_text(slot, &self.data)
                ));
            });
        }
        chosen.and_then(|choice| self.ask(choice))
    }

    /// The new game's continent, culture, name and wealth; returns the game to open.
    fn new_page(&mut self, ui: &mut egui::Ui) -> Option<ClubScreen> {
        ui.heading(tr!("title-new-game"));
        ui.add_space(6.0);
        let data = Rc::clone(&self.data);
        egui::Grid::new("new game")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label(tr!("new-continent"));
                let continent = self.new_continent;
                egui::ComboBox::from_id_salt("new continent")
                    .selected_text(continent_name(self.new_continent))
                    .show_ui(ui, |ui| {
                        for continent in Continent::ALL {
                            ui.selectable_value(
                                &mut self.new_continent,
                                continent,
                                continent_name(continent),
                            );
                        }
                    });
                ui.end_row();
                if self.new_continent != continent {
                    self.draw_new_club(None);
                }
                ui.label(tr!("new-culture"));
                let (culture, name) = &mut self.new_club;
                let before = culture.clone();
                egui::ComboBox::from_id_salt("new culture")
                    .selected_text(culture_name(culture))
                    .show_ui(ui, |ui| {
                        let here = data.cultures.iter().filter(|c| c.continent == continent);
                        for c in here {
                            ui.selectable_value(culture, c.key.clone(), culture_name(&c.key));
                        }
                    });
                ui.end_row();
                ui.label(tr!("new-name"));
                let mut draw = *culture != before;
                ui.horizontal(|ui| {
                    club_name_editor(ui, &data, culture, name);
                    draw |= ui.button(tr!("menu-draw-name")).clicked();
                });
                ui.end_row();
                if draw {
                    let culture = culture.clone();
                    self.draw_new_club(Some(&culture));
                }
            });
        ui.add_space(6.0);
        ui.label(tr!("new-wealth"));
        let mut chosen = None;
        ui.horizontal(|ui| {
            // A club needs a name.
            let named = !self.new_club.1.name.trim().is_empty();
            for wealth in Wealth::ALL {
                let button = egui::Button::new(tr!("menu-new-club", wealth = wealth_name(wealth)));
                if ui.add_enabled(named, button).clicked() {
                    chosen = Some(Confirm::NewGame(wealth, self.new_continent));
                }
            }
        });
        chosen.and_then(|choice| self.ask(choice))
    }

    /// Draws the new game's club a name in `culture`, or in a culture drawn on its
    /// continent.
    fn draw_new_club(&mut self, culture: Option<&str>) {
        let mut rng = ChaCha8Rng::seed_from_u64(new_seed());
        let key = match culture {
            Some(key) => Some(key.to_owned()),
            None => rivals::culture_on(&mut rng, Some(self.new_continent), &self.data),
        };
        let Some(culture) = key.as_deref().and_then(|key| self.data.culture(key)) else {
            return;
        };
        let name = names::club_name(&mut rng, culture, &[]);
        self.new_club = (culture.key.clone(), name);
    }

    /// The question asked before carrying out `choice`.
    fn question(&self, choice: Confirm) -> String {
        match choice {
            Confirm::Load(slot) => tr!(
                "confirm-load",
                slot = slot + 1,
                game = slot_text(&self.slots[slot], &self.data)
            ),
            Confirm::NewGame(wealth, continent) => tr!(
                "confirm-new-game",
                club = named_club(&self.new_club.1),
                wealth = wealth_name(wealth),
                continent = continent_name(continent)
            ),
        }
    }

    /// The question waiting for the player, « Confirmer » and « Annuler »; returns the
    /// game to open.
    fn confirmation(&mut self, ui: &mut egui::Ui, choice: Confirm) -> Option<ClubScreen> {
        ui.colored_label(egui::Color32::LIGHT_RED, self.question(choice));
        let mut opened = None;
        ui.horizontal(|ui| {
            if ui.button(tr!("confirm")).clicked() {
                opened = self.carry_out(choice);
            }
            if ui.button(tr!("cancel")).clicked() {
                self.confirm = None;
            }
        });
        opened
    }
}

impl Screen for TitleScreen {
    fn update(&mut self, ui: &egui::Context, _assets: &Assets, audio: &mut Audio) -> Transition {
        if std::mem::take(&mut self.stale) {
            self.read_files();
        }
        // Échap belongs to the options while they are open.
        // Échap cancels the question, else goes back to the first page, else quits.
        if !self.options_open
            && self.bindings.pressed(ui, &Action::Quit)
            && self.confirm.take().is_none()
        {
            if self.page == Page::Main {
                return Transition::Pop;
            }
            self.page = Page::Main;
        }
        let mut opened = None;
        let mut quit = false;
        egui::CentralPanel::default().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.heading(egui::RichText::new("Lanista").size(48.0));
                ui.add_space(30.0);
                if let Some(message) = &self.message {
                    ui.colored_label(egui::Color32::LIGHT_RED, message);
                    ui.add_space(6.0);
                }
                if let Some(choice) = self.confirm {
                    opened = self.confirmation(ui, choice);
                    return;
                }
                match self.page {
                    Page::Main => (opened, quit) = self.main_page(ui),
                    Page::Load => opened = self.load_page(ui),
                    Page::New => opened = self.new_page(ui),
                }
                if self.page != Page::Main {
                    ui.add_space(12.0);
                    if ui.button(tr!("back")).clicked() {
                        self.page = Page::Main;
                    }
                }
            });
        });
        if let Some(failed) =
            options::window(ui, audio, self.saves.as_deref(), &mut self.options_open)
        {
            self.message = Some(failed);
        }
        if let Some(club) = opened {
            return Transition::Push(Box::new(club));
        }
        if quit {
            return Transition::Pop;
        }
        Transition::None
    }

    fn draw(&self, _assets: &Assets) {
        clear_background(BACKGROUND);
    }
}

/// The pattern of the club's name among its culture's, and the name itself, written
/// freely.
fn club_name_editor(ui: &mut egui::Ui, data: &DesignData, culture: &str, name: &mut ClubName) {
    let patterns = data.culture(culture).map_or(&[][..], |c| &c.clubs[..]);
    egui::ComboBox::from_id_salt("new pattern")
        .selected_text(named_club(name))
        .show_ui(ui, |ui| {
            for pattern in patterns {
                let shown = ClubName {
                    pattern: pattern.clone(),
                    ..name.clone()
                };
                ui.selectable_value(&mut name.pattern, pattern.clone(), named_club(&shown));
            }
        });
    ui.add(egui::TextEdit::singleline(&mut name.name).desired_width(140.0));
}

/// What `file` in `folder` holds; empty without a folder.
fn read_slot(folder: Option<&Path>, file: &str, data: &DesignData) -> Slot {
    match folder.map(|f| saves::read(f, file, data)) {
        None | Some(Ok(None)) => Slot::Empty,
        Some(Ok(Some(state))) => Slot::Game(state.week, state.name, state.wealth, state.continent),
        Some(Err(_)) => Slot::Unreadable,
    }
}

/// What each save slot in `folder` holds; all empty without a folder.
pub fn read_slots(folder: Option<&Path>, data: &DesignData) -> [Slot; saves::SLOTS.len()] {
    saves::SLOTS.map(|file| read_slot(folder, file, data))
}

/// `slot` in a few words: empty, unreadable, or its club, date, wealth and continent.
pub fn slot_text(slot: &Slot, data: &DesignData) -> String {
    match slot {
        Slot::Empty => tr!("save-slot-empty"),
        Slot::Game(week, club, wealth, continent) => tr!(
            "save-slot-game",
            club = named_club(club),
            date = date_text(*week, data),
            wealth = wealth_name(*wealth),
            continent = continent_name(*continent)
        ),
        Slot::Unreadable => tr!("save-slot-unreadable"),
    }
}

/// A seed for a new game or a drawn name: the game, unlike `lanista_core`, may read
/// the clock.
fn new_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() ^ u64::from(d.subsec_nanos()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lanista_core::state::{Decisions, next_week};

    fn title(folder: Option<PathBuf>) -> TitleScreen {
        let data = Rc::new(DesignData::load().unwrap());
        TitleScreen::new(data, folder, Bindings::new(Vec::new()))
    }

    #[test]
    fn a_new_club_bears_the_name_chosen_or_drawn_in_a_culture_of_its_continent() {
        let mut title = title(None);
        // A club is drawn at once in a culture of the continent offered.
        let culture = title.data.culture(&title.new_club.0).unwrap();
        assert_eq!(culture.continent, title.new_continent);
        assert!(culture.names.contains(&title.new_club.1.name));
        // Another continent draws a culture of it and a name of that culture.
        title.new_continent = Continent::Africa;
        title.draw_new_club(None);
        let culture = title.data.culture(&title.new_club.0).unwrap();
        assert_eq!(culture.continent, Continent::Africa);
        assert!(culture.names.contains(&title.new_club.1.name));
        // The player writes its own name.
        title.new_club.1.name = "Spartacus".into();
        let chosen = title.new_club.clone();
        let club = title.new_game(Wealth::Rich, Continent::Africa);
        let state = club.state();
        assert_eq!((state.culture.clone(), state.name.clone()), chosen);
        assert_eq!(state.continent, Continent::Africa);
        let name = named_club(&chosen.1);
        assert!(name.contains("Spartacus"), "{name}");
        assert!(club.message().unwrap().contains(&name));
    }

    #[test]
    fn the_game_opened_is_the_one_continued_at_the_next_launch() {
        let data = Rc::new(DesignData::load().unwrap());
        let folder = std::env::temp_dir().join(format!("lanista-title-{}", std::process::id()));
        // Nothing to continue nor load at first.
        let mut first = title(Some(folder.clone()));
        assert_eq!(first.autosave, Slot::Empty);
        assert!(first.resume().is_none());
        // A new game is kept at once, then continued.
        let started = first
            .new_game(Wealth::Modest, Continent::Europe)
            .state()
            .clone();
        let mut again = title(Some(folder.clone()));
        let resumed = again.resume().unwrap();
        assert_eq!(resumed.state(), &started);
        assert_eq!(resumed.message(), Some("Partie reprise"));
        // A slot loaded becomes the game to continue, once the player confirms it.
        let mut older = GameState::new(7, &data);
        next_week(&mut older, &Decisions::default(), &data);
        saves::write(&folder, saves::SLOTS[1], &older).unwrap();
        again.read_files();
        assert!(again.ask(Confirm::Load(1)).is_none());
        assert_eq!(again.confirm, Some(Confirm::Load(1)));
        assert_eq!(
            title(Some(folder.clone())).resume().unwrap().state(),
            &started
        );
        let loaded = again.carry_out(Confirm::Load(1)).unwrap();
        assert_eq!((loaded.state(), again.confirm), (&older, None));
        assert_eq!(
            title(Some(folder.clone())).resume().unwrap().state(),
            &older
        );
        // A new game waits for the player too.
        again.read_files();
        assert!(
            again
                .ask(Confirm::NewGame(Wealth::Rich, Continent::Asia))
                .is_none()
        );
        std::fs::remove_dir_all(folder).unwrap();
    }
}
