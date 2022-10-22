use std::collections::HashMap;

use eframe::{
    egui::{self, ScrollArea},
};
use egui_extras::RetainedImage;
use poll_promise::Promise;
mod macros;
mod yugioh;
use rayon::prelude::*;
use yugioh::{YugiohCard, YugiohCardSearchCriteria, YugiohCards, YugiohDeck};

const CARD_HEIGHT: f32 = 128.0;
const ASPECT_RATIO: f32 = 2.25/3.25;
const CARD_WIDTH: f32 = CARD_HEIGHT * ASPECT_RATIO;
const CARD_MARGIN: f32 = 2.0;
const CARD_ROUNDING: f32 = 2.0;

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let native_options = eframe::NativeOptions::default();
        eframe::run_native(
            "Yugioh Deck Builder",
            native_options,
            Box::new(|cc| Box::new(App::new(cc))),
        );
    }
    #[cfg(target_arch = "wasm32")]
    {
        console_error_panic_hook::set_once();

        tracing_wasm::set_as_global_default();

        let web_options = eframe::WebOptions::default();
        eframe::start_web(
            "the_canvas_id",
            web_options,
            Box::new(|cc| Box::new(App::new(cc))),
        )
        .expect("Failed to start");
    }
}

pub struct App {
    p: Option<Promise<Vec<YugiohCard>>>,
    cards: Vec<YugiohCard>,
    deck: YugiohDeck,
    list_display_mode: ListDisplayMode,
    api_override: bool,
    search_criteria: YugiohCardSearchCriteria,
    last_search_criteria: YugiohCardSearchCriteria,
    search_results: Option<Vec<YugiohCard>>,
    cached_images: Vec<RetainedImage>,
    image_promises: HashMap<String, Promise<Result<RetainedImage, anyhow::Error>>>,
    buffers: Vec<String>,
    sorting: SortingMode,
    last_sorting: SortingMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListDisplayMode {
    Card,
    ImageOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SortingMode {
    stype: SortingType,
    order: Ord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortingType {
    Name,
    Id,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ord {
    Asc,
    Dsc,
}

impl App {
    pub fn new(_: &eframe::CreationContext<'_>) -> Self {
        App {
            p: None,
            cards: Vec::new(),
            deck: YugiohDeck::new(),
            list_display_mode: ListDisplayMode::Card,
            api_override: false,
            search_criteria: YugiohCardSearchCriteria::new(),
            search_results: None,
            last_search_criteria: YugiohCardSearchCriteria::new(),
            cached_images: Vec::new(),
            image_promises: HashMap::new(),
            buffers: vec![String::new(); 10],
            sorting: SortingMode {
                stype: SortingType::Name,
                order: Ord::Dsc,
            },
            last_sorting: SortingMode {
                stype: SortingType::Name,
                order: Ord::Dsc,
            },
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.cards.is_empty() {
            if self.p.is_none() {
                let api_override = self.api_override;
                self.p = Some(promisify!(async move {
                    // attempt to open the data.json file, if this fails, get the data from the api and save it to the file
                    let data: Result<String, anyhow::Error> = if api_override {
                        Err(anyhow::Error::msg("API override"))
                    } else {
                        let r = std::fs::read_to_string("data.json");
                        if let Ok(data) = r {
                            Ok(data)
                        } else {
                            Err(anyhow::Error::msg("Failed to read data.json"))
                        }
                    };
                    let cards: YugiohCards;
                    if let Ok(data) = data {
                        //println!("data.json found, loading data from file");
                        cards = serde_json::from_str(&data).unwrap();
                    } else {
                        //println!("data.json not found, loading data from api");
                        let data = reqwest::get("https://db.ygoprodeck.com/api/v7/cardinfo.php")
                            .await
                            .unwrap()
                            .text()
                            .await
                            .unwrap();
                        cards = serde_json::from_str(&data).unwrap();
                        std::fs::write("data.json", data).unwrap();
                    }
                    let mut parsed_cards = Vec::new();
                    for card in cards.data {
                        parsed_cards.push(YugiohCard::from_raw(card));
                    }
                    parsed_cards
                }));
            } else if self.p.as_ref().unwrap().ready().is_some() {
                self.cards = self.p.take().unwrap().block_and_take();
            }
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            if !self.cards.is_empty() {
                // TODO: add a deck view, expanding grid of cards
                ui.separator();
                // search bar for all card fields
                self.last_search_criteria = self.search_criteria.clone();
                self.last_sorting = self.sorting;
                ui.horizontal(|ui| {
                    ui.label("Search");
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            if ui.text_edit_singleline(&mut self.buffers[0]).changed() {
                                self.search_criteria.string = self.buffers[0].clone();
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Sorting");
                            ui.radio_value(&mut self.sorting.stype, SortingType::Name, "Name");
                            ui.radio_value(&mut self.sorting.stype, SortingType::Id, "Id");
                            ui.radio_value(&mut self.sorting.order, Ord::Asc, "Asc");
                            ui.radio_value(&mut self.sorting.order, Ord::Dsc, "Dsc");
                        });
                    });
                });
                if self.search_results.is_none() || self.last_search_criteria != self.search_criteria || self.last_sorting != self.sorting {
                    let criteria = self.search_criteria.clone();

                    let mut c = self.cards
                            .par_iter()
                            .filter(move |card| criteria.clone().matches(card))
                            .cloned()
                            .collect::<Vec<YugiohCard>>();

                    c.sort_by(match self.sorting {
                                SortingMode {
                                    stype: SortingType::Name,
                                    order: Ord::Asc,
                                } => |a: &YugiohCard, b: &YugiohCard| a.name.cmp(&b.name),
                                SortingMode {
                                    stype: SortingType::Name,
                                    order: Ord::Dsc,
                                } => |a: &YugiohCard, b: &YugiohCard| b.name.cmp(&a.name),
                                SortingMode {
                                    stype: SortingType::Id,
                                    order: Ord::Asc,
                                } => |a: &YugiohCard, b: &YugiohCard| a.id.cmp(&b.id),
                                SortingMode {
                                    stype: SortingType::Id,
                                    order: Ord::Dsc,
                                } => |a: &YugiohCard, b: &YugiohCard| b.id.cmp(&a.id),
                            });
                            c.reverse();

                    self.search_results = Some(
                        c
                    );
                }
                
                ui.separator();
                match self.list_display_mode {
                    ListDisplayMode::Card => {
                        ui.label("Card");
                        if ui.button("Image Only").clicked() {
                            self.list_display_mode = ListDisplayMode::ImageOnly;
                        }
                        ui.separator();
                        // card view method will be a scrollable list of card cards. these cards will contain a structured view of all of the card data including a lazy loaded thumbnail image of the card
                        if let Some(search_results) = self.search_results.as_mut() {
                            
                            ScrollArea::vertical().show_rows(
                                ui,
                                CARD_HEIGHT,
                                search_results.len(),
                                |ui, range| {
                                    let input_position = ui.input().pointer.hover_pos().unwrap_or(egui::Pos2 { x: ui.available_width(), y: ui.available_height() });
                                    let mut card_to_draw = None;
                                    for i in range {
                                        // create a rectangle of a slightly lighter color for the card, this will be the width of the window and 32. tall
                                        let rect = ui.allocate_space(egui::Vec2::new(
                                            ui.available_width(),
                                            CARD_HEIGHT + CARD_MARGIN,
                                        ));
                                        let card = &mut search_results[i];
                                        // create a rectangle for the image, this will be 30. tall and aspect ratio wide
                                        let image_rect = egui::Rect::from_min_max(
                                            rect.1.min + egui::Vec2::new(0., CARD_MARGIN),
                                            rect.1.min + egui::Vec2::new(CARD_WIDTH - CARD_MARGIN, CARD_HEIGHT - CARD_MARGIN),
                                        );

                                        // create a rectangle for the text, this will be 30. tall and the rest of the width
                                        let text_rect = egui::Rect::from_min_max(
                                            rect.1.min + egui::Vec2::new(CARD_WIDTH + CARD_MARGIN, CARD_MARGIN),
                                            rect.1.max - egui::Vec2::new(CARD_MARGIN, CARD_MARGIN),
                                        );
                                        // draw the image rectangle
                                        ui.painter().rect(
                                            image_rect,
                                            CARD_ROUNDING,
                                            egui::Color32::from_rgb(54, 54, 54),
                                            egui::Stroke::new(1., egui::Color32::from_rgb(64, 64, 64)),
                                        );
                                        // draw the text rectangle
                                        ui.painter().rect(
                                            text_rect,
                                            CARD_ROUNDING,
                                            egui::Color32::from_rgb(64, 64, 64),
                                            egui::Stroke::new(1., egui::Color32::from_rgb(64, 64, 64)),
                                        );
                                        // draw the image
                                        if let Some(_image) = card.card_image.image_small {
                                            // draw the image using the image_rect. this will be scaled to fit the rectangle via a mesh with uv
                                            let mut mesh = egui::Mesh::with_texture(
                                                *(card.clone()).card_image.image_small.as_ref().unwrap(),
                                                );
                                            mesh.add_rect_with_uv(
                                                image_rect,
                                                egui::Rect::from_min_max(
                                                    egui::Pos2::new(0., 0.),
                                                    egui::Pos2::new(1., 1.),
                                                ),
                                                egui::Color32::WHITE,
                                            );
                                            ui.painter().add(egui::Shape::Mesh(mesh));

                                        } else {
                                            // if the image is not cached, create a promise to get the image and add it to the cache
                                            if !self.image_promises.contains_key(
                                                format!("small:{}", &card.id).as_str(),
                                            ) {
                                                let id = card.id;
                                                let api_override = self.api_override;
                                                let cardmove = card.clone();
                                                self.image_promises.insert(
                                                    format!("small:{}", &cardmove.id),
                                                    promisify!(async move {
                                                        // first attempt to load the image from the cache. the cache is /images/small/{id}.cache
                                                        let mut traceback = String::new();
                                                        let image_bytes: Result<
                                                            Vec<u8>,
                                                            anyhow::Error,
                                                        > = if api_override {
                                                            Err(anyhow::Error::msg("API override"))
                                                        } else {
                                                            let r = std::fs::read(format!(
                                                                "./cache/small/{}.cache",
                                                                id
                                                            ));
                                                            if let Ok(data) = r {
                                                                Ok(data)
                                                            } else {
                                                                Err(anyhow::Error::msg(
                                                                    "Failed to read image",
                                                                ))
                                                            }
                                                        };
                                                        // if the image is not in the cache, attempt to load it from the api
                                                        if let Ok(image_bytes) = image_bytes {
                                                            let image = RetainedImage::from_image_bytes(format!("small:{}", id), &image_bytes[..]);
                                                            if let Ok(image) = image {
                                                                Ok(image)
                                                            } else {
                                                                // delete the image from the cache if it is corrupted
                                                                let res = std::fs::remove_file(format!("./cache/small/{}.cache", id));
                                                                // add the traceback to the error
                                                                if res.is_ok() {
                                                                    traceback.push_str("Failed to load image bytes\n");
                                                                } else {
                                                                    traceback.push_str("Failed to load image bytes\nFailed to delete corrupted image from cache\n");
                                                                }
                                                                
                                                                Err(anyhow::anyhow!(traceback))
                                                            }
                                                        } else {
                                                            traceback.push_str("Failed to read image from cache, attempting to load from api\n");
                                                            let image = reqwest::get(cardmove.card_image.image_url_small.as_str()).await;
                                                            if let Ok(image) = image {
                                                                let image_bytes = image.bytes().await;
                                                                if let Ok(image_bytes) = image_bytes {
                                                                    let raw_image_bytes = image_bytes.to_vec();
                                                                    let res = std::fs::write(
                                                                        format!("./cache/small/{}.cache", id),
                                                                        &raw_image_bytes,
                                                                    );
                                                                    if let Err(res) = res {
                                                                        traceback.push_str(format!("Failed to write image to cache: {}\n", res).as_str());
                                                                    }
                                                                    let image = RetainedImage::from_image_bytes(format!("small:{}", id), &raw_image_bytes.to_vec()[..]);
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
                                                        
                                                    }),
                                                );
                                                //println!("Created promise for small image: {}", id);
                                            } else {
                                                // if the image is already being loaded, draw a loading indicator
                                                ui.painter().text(
                                                    image_rect.center(),
                                                    egui::Align2::CENTER_CENTER,
                                                    "Loading...",
                                                    egui::FontId::default(),
                                                    egui::Color32::from_rgb(0, 0, 0),
                                                );
                                                // and check if the image promise has been fulfilled
                                                
                                                let im = self.image_promises.remove(format!("small:{}", &card.id).as_str()).unwrap().try_take();
                                                // if the image promise has been fulfilled, remove it from the cache
                                                // and add the image to the cache
                                                if let Ok(Ok(im)) = im {
                                                    card.card_image.image_small = Some(im.texture_id(ctx));
                                                    self.cached_images.push(im);
                                                } else {
                                                    //eprintln!("Failed to load image: {}", im.err().unwrap());
                                                }
                                               
                                            }
                                        }
                                        if image_rect.contains(input_position) {
                                            card_to_draw = Some((card.clone(), image_rect));
                                        }
                                    }
                                    if let Some((card, image_rect)) = card_to_draw {
                                        // if we are hovering over the small image, attempt to draw a large image, lazy loading it like we do for the small image
                                        let card = self.cards.iter_mut().find(|c| c.id == card.id).unwrap();
                                        if let Some(bigimage) = card.card_image.image {
                                            // if we have the image, draw it, the min being the cursor position
                                            // the max being the cursor position + either the width of the window or the height of the window, whichever is smaller in relation to the aspect ratio of the image ASPECT_RATIO
                                            
                                            let min_x = input_position.x;
                                            let ui_rect = ui.clip_rect();
                                            // determine if the image should be drawn at the height of the window or the width of the window
                                            let max_x = ui_rect.max.y * ASPECT_RATIO;
                                            let bigimage_rect = if max_x < ui_rect.max.x - min_x {
                                                // if the width of the image WOULD NOT be larger than the available width of the window
                                                
                                                // then the min_y will be the top of the window
                                                let min_y = ui_rect.min.y;
                                                // and the max_y will be the bottom of the window
                                                let max_y = ui_rect.max.y;
                                                // and the width will be the height of the window * the aspect ratio
                                                let max_x = input_position.x + ((max_y - min_y) * ASPECT_RATIO);

                                                
                                                egui::Rect::from_min_max(
                                                    egui::Pos2::new(min_x, min_y),
                                                    egui::Pos2::new(max_x, max_y),
                                                )
                                            } else {
                                                // if the width of the image WOULD be larger than the available width of the window
                                                if input_position.y < ui_rect.center().y {
                                                    // if the cursor is in the top half of the window

                                                    // then the min_y will be the top of the window
                                                    let min_y = ui_rect.min.y;
                                                    // and the max_x will be the right of the window
                                                    let max_x = ui_rect.max.x;
                                                    // and the max_y will be scaled with the aspect ratio
                                                    let max_y = min_y + (max_x - min_x) / ASPECT_RATIO;
                                                    egui::Rect::from_min_max(
                                                        egui::Pos2::new(min_x, min_y),
                                                        egui::Pos2::new(max_x, max_y),
                                                    )
                                                } else {
                                                    // if the cursor is in the bottom half of the window

                                                    // then the max_y will be the bottom of the window
                                                    let max_y = ui_rect.max.y;
                                                    // and the max_x will be the right of the window
                                                    let max_x = ui_rect.max.x;
                                                    // and the min_y will be scaled with the aspect ratio
                                                    let min_y = max_y - (max_x - min_x) / ASPECT_RATIO;
                                                    egui::Rect::from_min_max(
                                                        egui::Pos2::new(min_x, min_y),
                                                        egui::Pos2::new(max_x, max_y),
                                                    )
                                                }
                                                    
                                            };


                                            let mut mesh = egui::Mesh::with_texture(bigimage);
                                            mesh.add_rect_with_uv(
                                                bigimage_rect,
                                                egui::Rect::from_min_max(
                                                    egui::Pos2::ZERO,
                                                    egui::Pos2::new(1., 1.),
                                                ),
                                                egui::Color32::WHITE,
                                            );
                                            ui.painter().add(egui::Shape::Mesh(mesh));
                                            
                                        } else if !self.image_promises.contains_key(
                                            format!("large:{}", &card.id).as_str(),
                                        ) {
                                            let card = card.clone();
                                            let id = card.id;
                                            let api_override = self.api_override;
                                            self.image_promises.insert(
                                                format!("large:{}", &card.id),
                                                promisify!(async move {
                                                    // first attempt to load the image from the cache. the cache is /images/large/{id}.cache
                                                    let mut traceback = String::new();
                                                    let image_bytes: Result<
                                                        Vec<u8>,
                                                        anyhow::Error,
                                                    > = if api_override {
                                                        Err(anyhow::Error::msg("API override"))
                                                    } else {
                                                        let r = std::fs::read(format!(
                                                            "./cache/large/{}.cache",
                                                            id
                                                        ));
                                                        if let Ok(data) = r {
                                                            Ok(data)
                                                        } else {
                                                            Err(anyhow::Error::msg(
                                                                "Failed to read image",
                                                            ))
                                                        }
                                                    };
                                                    // if the image is not in the cache, attempt to load it from the api
                                                    if let Ok(image_bytes) = image_bytes {
                                                        let image = RetainedImage::from_image_bytes(format!("large:{}", id), &image_bytes[..]);
                                                        if let Ok(image) = image {
                                                            Ok(image)
                                                        } else {
                                                            // delete the image from the cache if it is corrupted
                                                            let res = std::fs::remove_file(format!("./cache/large/{}.cache", id));
                                                            // add the traceback to the error
                                                            if res.is_ok() {
                                                                traceback.push_str("Failed to load image bytes\n");
                                                            } else {
                                                                traceback.push_str("Failed to load image bytes\nFailed to delete corrupted image from cache\n");
                                                            }
                                                            
                                                            Err(anyhow::anyhow!(traceback))
                                                        }
                                                    } else {
                                                        traceback.push_str("Failed to read image from cache, attempting to load from api\n");
                                                        let image = reqwest::get(card.card_image.image_url.as_str()).await;
                                                        if let Ok(image) = image {
                                                            let image_bytes = image.bytes().await;
                                                            if let Ok(image_bytes) = image_bytes {
                                                                let raw_image_bytes = image_bytes.to_vec();
                                                                let res = std::fs::write(
                                                                    format!("./cache/large/{}.cache", id),
                                                                    &raw_image_bytes,
                                                                );
                                                                if let Err(res) = res {
                                                                    traceback.push_str(format!("Failed to write image to cache: {}\n", res).as_str());
                                                                }
                                                                let image = RetainedImage::from_image_bytes(format!("large:{}", id), &raw_image_bytes.to_vec()[..]);
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
                                                    
                                                }),
                                            );
                                            //println!("Created promise for large image: {}", id);
                                        } else {
                                            // if the image is already being loaded, draw a loading indicator
                                            ui.painter().text(
                                                image_rect.center(),
                                                egui::Align2::CENTER_CENTER,
                                                "Loading...",
                                                egui::FontId::default(),
                                                egui::Color32::from_rgb(0, 0, 0),
                                            );
                                            // and check if the image promise has been fulfilled
                                            if let Some(image) = self.image_promises.get(format!("large:{}", &card.id).as_str()) {
                                                if image.ready().is_some() {
                                                    // take ownership of the image from the hashmap
                                                    let im = self.image_promises.remove(format!("large:{}", &card.id).as_str()).unwrap().block_and_take();
                                                    // if the image promise has been fulfilled, remove it from the cache
                                                    // and add the image to the cache
                                                    if let Ok(im) = im {
                                                        card.card_image.image = Some(im.texture_id(ctx));
                                                        self.cached_images.push(im);
                                                    } else {
                                                        //eprintln!("Failed to load image: {}", im.err().unwrap());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                },
                            );
                        }
                    }
                    ListDisplayMode::ImageOnly => {
                        ui.label("Image Only");
                        if ui.button("Card").clicked() {
                            self.list_display_mode = ListDisplayMode::Card;
                        }
                    }
                }
            } else {
                ui.spinner();
            }
        });
    }
}
