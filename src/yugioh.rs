use eframe::epaint::TextureId;

use serde::Deserialize;

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

#[derive(Clone)]
pub struct YugiohImage {
    pub url: String,
    pub image: Option<TextureId>,
    pub promise_index: Option<usize>,
}

impl YugiohImage {
    pub fn from_raw(url: String) -> Self {
        Self {
            url,
            image: None,
            promise_index: None,
        }
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
            small: YugiohImage::from_raw(raw_card_image.image_url_small),
            large: YugiohImage::from_raw(raw_card_image.image_url),
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

#[derive(Debug)]
pub struct YugiohDeck {
    pub main_deck: Vec<YugiohCard>,
    pub extra_deck: Vec<YugiohCard>,
    pub side_deck: Vec<YugiohCard>,
}

impl YugiohDeck {
    pub fn new() -> Self {
        Self {
            main_deck: Vec::new(),
            extra_deck: Vec::new(),
            side_deck: Vec::new(),
        }
    }
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
        card.name.to_lowercase().contains(self.string.to_lowercase().as_str())
    }
}
