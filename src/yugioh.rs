use std::{io::BufRead, path::PathBuf};

use anyhow::anyhow;
use eframe::epaint::TextureId;

use egui_extras::RetainedImage;
use poll_promise::Promise;
use serde::Deserialize;
use wildmatch::WildMatch;

use crate::sizedbuffer::Buffer;

#[derive(Debug, Deserialize, Clone)]
pub struct YugiohCards {
    pub data: Vec<RawYugiohCard>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RawYugiohCard {
    pub id: u32,
    pub name: String,
    #[serde(alias = "type")]
    pub card_type: String,
    pub desc: String,
    pub race: String,
    pub archetype: Option<String>,
    pub card_sets: Option<Vec<RawCardSet>>,
    pub card_images: Vec<RawCardImage>,
    pub card_prices: Vec<RawCardPrice>,
}
#[derive(Debug, Deserialize, Clone)]
pub struct RawCardSet {
    pub set_name: String,
    pub set_code: String,
    pub set_rarity: String,
    pub set_rarity_code: String,
    pub set_price: String,
}
#[derive(Debug, Deserialize, Clone)]
pub struct RawCardImage {
    pub id: u32,
    pub image_url: String,
    pub image_url_small: String,
}
#[derive(Debug, Deserialize, Clone)]
pub struct RawCardPrice {
    pub cardmarket_price: String,
    pub tcgplayer_price: String,
    pub ebay_price: String,
    pub amazon_price: String,
    pub coolstuffinc_price: String,
}

// parsed form of RawYugiohCard
#[derive(Debug, Clone)]
pub struct YugiohCard {
    pub id: u32,
    pub name: String,
    pub card_type: String,
    pub desc: String,
    pub race: String,
    pub archetype: String,
    pub card_sets: Option<Vec<CardSet>>,
    pub card_image: CardImage,
    pub card_prices: CardPrice,
}

#[derive(Debug, Clone)]
pub struct CardSet {
    pub set_name: String,
    pub set_code: String,
    pub set_rarity: String,
    pub set_rarity_code: String,
    pub set_price: f32,
}

#[derive(Clone, Debug)]
pub struct CardImage {
    pub id: u32,
    pub small: YugiohImage,
    pub large: YugiohImage,
}

impl CardImage {
    pub fn check_promises(
        &mut self,
        ctx: &eframe::egui::Context,
        promises: &mut Buffer<Promise<Result<RetainedImage, anyhow::Error>>>,
        not_ready_is_err: bool,
    ) -> [(bool, Result<Option<RetainedImage>, anyhow::Error>); 2] {
        [self.small.check_promise(ctx, promises, not_ready_is_err), self.large.check_promise(ctx, promises, not_ready_is_err)]
    }
}

#[derive(Clone)]
pub struct YugiohImage {
    pub id: u32,
    pub url: String,
    pub image: Option<TextureId>,
    pub promise_index: Option<usize>,
    pub size: String,
}

impl YugiohImage {
    pub fn from_raw(url: String, id: u32, size: String) -> Self {
        Self {
            url,
            image: None,
            promise_index: None,
            id,
            size,
        }
    }

    pub fn check_promise(
        &mut self,
        ctx: &eframe::egui::Context,
        promises: &mut Buffer<Promise<Result<RetainedImage, anyhow::Error>>>,
        not_ready_is_err: bool,
    ) -> (bool, Result<Option<RetainedImage>, anyhow::Error>) {
        if let Some(promise_index) = self.promise_index {
            let im = promises.get_ref(promise_index);
            if let Some(im) = im {
                if im.poll().is_ready() {
                    let im = promises.try_take(promise_index);
                    if let Some(im) = im {
                        let im = im.block_and_take();
                        self.promise_index = None;
                        if let Ok(im) = im {
                            self.image = Some(im.texture_id(ctx));
                            (true, Ok(Some(im)))
                        } else {
                            (true, Err(anyhow!("{:?}\n{:?}", im.err().unwrap(), "Image promise exists but was not fulfilled")))
                        }
                    } else {
                        (true, Err(anyhow!("{:?}", "Image promise could not be taken")))
                    }
                } else if not_ready_is_err {
                    (true, Err(anyhow!("{:?}", "Image promise is not ready")))
                } else {
                    (false, Ok(None))
                }
            } else {
                self.promise_index = None;
                (true, Err(anyhow!("{:?}", "Image promise does not exist")))
            }
        } else {
            (false, Ok(None))
        }
    }

    pub fn get_promise(&self, api_override: bool, cache_path_raw: PathBuf) -> Promise<Result<RetainedImage, anyhow::Error>> {
        let cache_path = cache_path_raw.join(format!("{}.cache", self.id));
        let url = self.url.clone();
        let debug_name = format!("{}:{}", self.size, self.id);
        Promise::spawn_thread(format!("{}:{}", self.size, self.id), move || {
            let mut traceback = String::new();
            let image_bytes: Result<Vec<u8>, anyhow::Error> = if api_override {
                Err(anyhow::Error::msg("API override"))
            } else {
                let r = std::fs::read(cache_path.clone());
                if let Ok(data) = r {
                    Ok(data)
                } else {
                    Err(anyhow::Error::msg("Failed to read image"))
                }
            };
            if let Ok(image_bytes) = image_bytes {
                let image = RetainedImage::from_image_bytes(debug_name, &image_bytes[..]);
                if let Ok(image) = image {
                    Ok(image)
                } else {
                    let res = std::fs::remove_file(cache_path);
                    if res.is_ok() {
                        traceback.push_str("Failed to load image bytes\n");
                    } else {
                        traceback.push_str("Failed to load image bytes\nFailed to delete corrupted image from cache\n");
                    }
                    Err(anyhow::anyhow!(traceback))
                }
            } else {
                traceback.push_str("Failed to read image from cache, attempting to load from api\n");
                let image = reqwest::blocking::get(url.as_str());
                if let Ok(image) = image {
                    let image_bytes = image.bytes();
                    if let Ok(image_bytes) = image_bytes {
                        let raw_image_bytes = image_bytes.to_vec();
                        let res = std::fs::write(cache_path, &raw_image_bytes);
                        if let Err(res) = res {
                            traceback.push_str(format!("Failed to write image to cache:{}\n", res).as_str());
                        }
                        let image = RetainedImage::from_image_bytes(debug_name, &raw_image_bytes.to_vec()[..]);
                        if let Ok(image) = image {
                            Ok(image)
                        } else {
                            traceback.push_str("Failed to load image bytes\n");
                            Err(anyhow::anyhow!(traceback))
                        }
                    } else {
                        traceback.push_str("Failed to read image bytes from api\n");
                        Err(anyhow::anyhow!(traceback))
                    }
                } else {
                    traceback.push_str("Failed to load image from api\n");
                    Err(anyhow::anyhow!(traceback))
                }
            }
        })
    }
}

impl std::fmt::Debug for YugiohImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CardImage")
            .field("url", &self.url)
            .field("image", &self.image.is_some())
            .field("promise_index", &self.promise_index)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct CardPrice {
    pub cardmarket_price: f32,
    pub tcgplayer_price: f32,
    pub ebay_price: f32,
    pub amazon_price: f32,
    pub coolstuffinc_price: f32,
}

impl YugiohCard {
    pub fn from_raw(raw_card: RawYugiohCard) -> Self {
        let card_sets = if let Some(card_sets) = raw_card.card_sets {
            let mut parsed_card_sets = Vec::new();
            for card_set in card_sets {
                parsed_card_sets.push(CardSet::from_raw(card_set));
            }

            Some(parsed_card_sets)
        } else {
            None
        };

        let card_image = CardImage::from_raw(raw_card.card_images[0].clone());
        let card_prices = CardPrice::from_raw(raw_card.card_prices[0].clone());

        Self {
            id: raw_card.id,
            name: raw_card.name,
            card_type: raw_card.card_type,
            desc: raw_card.desc,
            card_image,
            card_prices,
            card_sets,
            race: raw_card.race,
            archetype: raw_card.archetype.unwrap_or_else(|| "None".to_string()),
        }
    }
    pub fn as_mut(&mut self) -> &mut Self {
        self
    }
}

impl CardSet {
    pub fn from_raw(raw_card_set: RawCardSet) -> Self {
        Self {
            set_name: raw_card_set.set_name,
            set_code: raw_card_set.set_code,
            set_rarity: raw_card_set.set_rarity,
            set_rarity_code: raw_card_set.set_rarity_code,
            set_price: raw_card_set.set_price.parse().unwrap_or(0.0),
        }
    }
}

impl CardImage {
    pub fn from_raw(raw_card_image: RawCardImage) -> Self {
        Self {
            id: raw_card_image.id,
            small: YugiohImage::from_raw(raw_card_image.image_url_small, raw_card_image.id, "small".to_owned()),
            large: YugiohImage::from_raw(raw_card_image.image_url, raw_card_image.id, "large".to_owned()),
        }
    }
}

impl CardPrice {
    pub fn from_raw(raw_card_price: RawCardPrice) -> Self {
        Self {
            cardmarket_price: raw_card_price.cardmarket_price.parse().unwrap_or(0.0),
            tcgplayer_price: raw_card_price.tcgplayer_price.parse().unwrap_or(0.0),
            ebay_price: raw_card_price.ebay_price.parse().unwrap_or(0.0),
            amazon_price: raw_card_price.amazon_price.parse().unwrap_or(0.0),
            coolstuffinc_price: raw_card_price.coolstuffinc_price.parse().unwrap_or(0.0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct YugiohDeck {
    pub main_deck: Vec<usize>,
    pub extra_deck: Vec<usize>,
    pub side_deck: Vec<usize>,
    pub been_loaded: bool,
}

impl YugiohDeck {
    pub fn new(been_loaded: bool) -> Self {
        Self {
            main_deck: Vec::new(),
            extra_deck: Vec::new(),
            side_deck: Vec::new(),
            been_loaded,
        }
    }
    pub fn from_file(path: PathBuf, cards: &[YugiohCard]) -> Self {
        // try to open the file
        let file = std::fs::File::open(path);
        if let Ok(file) = file {
            let mut current_deck = DeckType::None;
            // read the file line by line
            let mut reader = std::io::BufReader::new(file);
            let mut main_deck = Vec::new();
            let mut extra_deck = Vec::new();
            let mut side_deck = Vec::new();
            let mut resultish = 1;
            while resultish != 0 {
                let mut str = String::new();
                let res = reader.read_line(&mut str);
                if let Ok(res) = res {
                    resultish = res;
                    // parse the line as a u32
                    let card_id = str.trim().parse::<u32>();
                    if let Ok(card_id) = card_id {
                        // get the index of the card in the cards vector
                        let card_index = cards.iter().position(|card| card.id == card_id);
                        if let Some(card_index) = card_index {
                            // add the card to the deck
                            match current_deck {
                                DeckType::Main => main_deck.push(card_index),
                                DeckType::Extra => extra_deck.push(card_index),
                                DeckType::Side => side_deck.push(card_index),
                                DeckType::None => (),
                            }
                        } else {
                            eprintln!("Card with id {} not found", card_id);
                        }
                    } else {
                        // match on the deck markers. if the line *contains* a deck marker, set the current deck to that deck
                        if str.contains("#main") {
                            current_deck = DeckType::Main;
                        } else if str.contains("#extra") {
                            current_deck = DeckType::Extra;
                        } else if str.contains("!side") {
                            current_deck = DeckType::Side;
                        }
                    }
                } else {
                    Self::new(true);
                }
            }
            Self {
                main_deck,
                extra_deck,
                side_deck,
                been_loaded: true,
            }
        } else {
            Self::new(true)
        }
    }

    pub fn contains_card(&self, card: usize) -> DeckType {
        if self.main_deck.contains(&card) {
            DeckType::Main
        } else if self.extra_deck.contains(&card) {
            DeckType::Extra
        } else if self.side_deck.contains(&card) {
            DeckType::Side
        } else {
            DeckType::None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeckType {
    None,
    Main,
    Extra,
    Side,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YugiohCardSearchCriteria {
    pub string: String,
}

impl YugiohCardSearchCriteria {
    pub fn new() -> Self {
        Self { string: String::new() }
    }
}

impl YugiohCardSearchCriteria {
    pub fn matches(self, card: &YugiohCard) -> bool {
        // check if a card matches the criteria
        // for the time being just check if the name contains the string
        match_wild(self.string.to_lowercase(), card.name.to_lowercase())
    }
}

fn match_wild(wild: String, string: String) -> bool {
    // if the string does not end with an !, add a wildcard to the end
    let mut wild = wild;
    if wild.ends_with('!') {
        wild.pop();
    } else {
        wild.push('*');
    }

    if wild.starts_with('!') {
        wild.remove(0);
    } else {
        wild.insert(0, '*');
    }
    // replace - in both strings with a space
    let mut special_chars = vec!['-', '(', ')', '[', ']', '{', '}', '+', '.', '\\', '^', '$', '|', '"', '\'', '!'];
    // if the wild string contains any of the special chars, remove them from the special chars list
    for c in wild.chars() {
        if special_chars.contains(&c) {
            special_chars.retain(|&x| x != c);
        }
    }
    let wild = clean_string(wild, special_chars.clone());
    let string = clean_string(string, special_chars);
    // println!("wild: {} | string: {}", wild, string);
    WildMatch::new(&wild).matches(&string)
}

fn clean_string(string: String, special_chars: Vec<char>) -> String {
    let mut string = string;
    for char in special_chars.iter() {
        string = string.replace(*char, "");
    }
    string
}
