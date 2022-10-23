use std::time::Instant;

use eframe::egui::{self, ScrollArea};
use egui_extras::RetainedImage;
use poll_promise::Promise;
// mod macros;
mod sizedbuffer;
mod yugioh;
use sizedbuffer::Buffer;
use yugioh::{YugiohCard, YugiohCardSearchCriteria, YugiohCards, YugiohDeck};
const CARD_HEIGHT: f32 = 128.0;
const ASPECT_RATIO: f32 = 2.25 / 3.25;
const CARD_WIDTH: f32 = CARD_HEIGHT * ASPECT_RATIO;
const CARD_MARGIN: f32 = 1.0;
const CARD_ROUNDING: f32 = 2.0;
const MAX_BUFFER_SIZE: usize = 100;
fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let native_options = eframe::NativeOptions::default();
        eframe::run_native("Yugioh Deck Builder", native_options, Box::new(|cc| Box::new(App::new(cc))));
    }
    #[cfg(target_arch = "wasm32")]
    {
        console_error_panic_hook::set_once();

        tracing_wasm::set_as_global_default();

        let web_options = eframe::WebOptions::default();
        eframe::start_web("the_canvas_id", web_options, Box::new(|cc| Box::new(App::new(cc)))).expect("Failed to start");
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
    search_results: Option<Vec<(usize, YugiohCard)>>,
    cached_images: Vec<RetainedImage>,
    image_promises: Buffer<Promise<Result<RetainedImage, anyhow::Error>>>,
    buffers: Vec<String>,
    sorting: SortingMode,
    last_sorting: SortingMode,
    request_repaint: bool,
    ppp: f32,
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
            list_display_mode: ListDisplayMode::ImageOnly,
            api_override: false,
            search_criteria: YugiohCardSearchCriteria::new(),
            search_results: None,
            last_search_criteria: YugiohCardSearchCriteria::new(),
            cached_images: Vec::new(),
            image_promises: Buffer::new(MAX_BUFFER_SIZE),
            buffers: vec![String::new(); 10],
            sorting: SortingMode {
                stype: SortingType::Name,
                order: Ord::Dsc,
            },
            last_sorting: SortingMode {
                stype: SortingType::Name,
                order: Ord::Dsc,
            },
            request_repaint: false,
            ppp: 1.0,
        }
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // println!("update");
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.cards.is_empty() {
                if self.p.is_none() {
                    let api_override = self.api_override;
                    self.p = Some(Promise::spawn_thread("data", move || {
                        let data: Result<String, anyhow::Error> = if api_override {
                            Err(anyhow::Error::msg("API override"))
                        } else {
                            let r = std::fs::read_to_string("./cache/data.json");
                            if let Ok(data) = r {
                                Ok(data)
                            } else {
                                Err(anyhow::Error::msg("Failed to read data.json"))
                            }
                        };
                        let cards: YugiohCards;
                        if let Ok(data) = data {
                            cards = serde_json::from_str(&data).unwrap();
                        } else {
                            let data = reqwest::blocking::get("https://db.ygoprodeck.com/api/v7/cardinfo.php").unwrap().text().unwrap();
                            cards = serde_json::from_str(&data).unwrap();
                            std::fs::write("./cache/data.json", data).unwrap();
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
                ui.spinner();
            } else {
                let r = ui.add(egui::Slider::new(&mut self.ppp, 1.0..=10.0).text("pixels per point"));
                if !r.dragged() {
                    ctx.set_pixels_per_point(self.ppp);
                }
                ui.separator();
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
                    let mut c: Vec<(usize, YugiohCard)> = Vec::new();
                    for (i, card) in self.cards.iter().enumerate() {
                        if criteria.clone().matches(card) {
                            c.push((i, card.clone()));
                        }
                    }

                    match self.sorting {
                        SortingMode {
                            stype: SortingType::Name,
                            order: Ord::Asc,
                        } => {
                            c.sort_by(|a, b| a.1.name.cmp(&b.1.name));
                        }
                        SortingMode {
                            stype: SortingType::Name,
                            order: Ord::Dsc,
                        } => {
                            c.sort_by(|a, b| b.1.name.cmp(&a.1.name));
                        }
                        SortingMode {
                            stype: SortingType::Id,
                            order: Ord::Asc,
                        } => {
                            c.sort_by(|a, b| a.1.id.cmp(&b.1.id));
                        }
                        SortingMode {
                            stype: SortingType::Id,
                            order: Ord::Dsc,
                        } => {
                            c.sort_by(|a, b| b.1.id.cmp(&a.1.id));
                        }
                    };
                    c.reverse();
                    self.search_results = Some(c);
                }
                ui.separator();
                match self.list_display_mode {
                    ListDisplayMode::Card => {
                        ui.label("Card");
                        if ui.button("Image Only").clicked() {
                            self.list_display_mode = ListDisplayMode::ImageOnly;
                        }
                        ui.separator();
                        if let Some(search_results) = self.search_results.as_mut() {
                            ScrollArea::vertical().show_rows(ui, CARD_HEIGHT, search_results.len(), |ui, range| {
                                let input_position = ui.input().pointer.hover_pos();
                                let mut card_to_draw = None;

                                for i in range {
                                    let rect = ui.allocate_space(egui::Vec2::new(ui.available_width(), CARD_HEIGHT + CARD_MARGIN));
                                    let card = self.cards[search_results[i].0].as_mut();
                                    let image_rect = egui::Rect::from_min_max(
                                        rect.1.min + egui::Vec2::new(0., CARD_MARGIN),
                                        rect.1.min + egui::Vec2::new(CARD_WIDTH - CARD_MARGIN, CARD_HEIGHT - CARD_MARGIN),
                                    );
                                    let text_rect = egui::Rect::from_min_max(
                                        rect.1.min + egui::Vec2::new(CARD_WIDTH + CARD_MARGIN, CARD_MARGIN),
                                        rect.1.max - egui::Vec2::new(CARD_MARGIN, CARD_MARGIN),
                                    );
                                    ui.painter().rect(
                                        image_rect,
                                        CARD_ROUNDING,
                                        egui::Color32::from_rgb(54, 54, 54),
                                        egui::Stroke::new(1., egui::Color32::from_rgb(64, 64, 64)),
                                    );
                                    ui.painter().rect(
                                        text_rect,
                                        CARD_ROUNDING,
                                        egui::Color32::from_rgb(64, 64, 64),
                                        egui::Stroke::new(1., egui::Color32::from_rgb(64, 64, 64)),
                                    );
                                    if let Some(image) = card.card_image.small.image {
                                        let mut mesh = egui::Mesh::with_texture(image);
                                        mesh.add_rect_with_uv(image_rect, egui::Rect::from_min_max(egui::Pos2::new(0., 0.), egui::Pos2::new(1., 1.)), egui::Color32::WHITE);
                                        ui.painter().add(egui::Shape::Mesh(mesh));
                                    } else if let Some(_promise_index) = card.card_image.small.promise_index {
                                        ui.painter().text(
                                            image_rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            "Loading...",
                                            egui::FontId::default(),
                                            egui::Color32::from_rgb(0, 0, 0),
                                        );
                                        self.request_repaint = true;
                                        // let im = self.image_promises.get_ref(promise_index);
                                        // if let Some(im) = im {
                                        //     if im.poll().is_ready() {
                                        //         let im = self.image_promises.try_take(promise_index);
                                        //         if let Some(im) = im {
                                        //             let im = im.block_and_take();
                                        //             if let Ok(im) = im {
                                        //                 card.card_image.small.image = Some(im.texture_id(ctx));
                                        //                 card.card_image.small.promise_index = None;
                                        //                 self.cached_images.push(im);
                                        //             } else {
                                        //                 eprintln!("Image promise exists but was not fulfilled");
                                        //             }
                                        //         } else {
                                        //             eprintln!("Image promise could not be taken");
                                        //         }
                                        //     } else {
                                        //         //eprintln!("Image promise is not ready");
                                        //     }
                                        // } else {
                                        //     eprintln!("Image promise does not exist");
                                        //     card.card_image.small.promise_index = None;
                                        // }
                                    } else {
                                        let movecard = card.clone();
                                        let id = card.id;
                                        let api_override = self.api_override;
                                        if self.image_promises.get_index().is_some() {
                                            let i = self.image_promises.try_add(Promise::spawn_thread(format!("small:{}", card.id), move || {
                                                let mut traceback = String::new();
                                                let image_bytes: Result<Vec<u8>, anyhow::Error> = if api_override {
                                                    Err(anyhow::Error::msg("API override"))
                                                } else {
                                                    let r = std::fs::read(format!("./cache/small/{}.cache", id));
                                                    if let Ok(data) = r {
                                                        Ok(data)
                                                    } else {
                                                        Err(anyhow::Error::msg("Failed to read image"))
                                                    }
                                                };
                                                if let Ok(image_bytes) = image_bytes {
                                                    let image = RetainedImage::from_image_bytes(format!("small:{}", id), &image_bytes[..]);
                                                    if let Ok(image) = image {
                                                        Ok(image)
                                                    } else {
                                                        let res = std::fs::remove_file(format!("./cache/small/{}.cache", id));
                                                        if res.is_ok() {
                                                            traceback.push_str("Failed to load image bytes\n");
                                                        } else {
                                                            traceback.push_str("Failed to load image bytes\nFailed to delete corrupted image from cache\n");
                                                        }
                                                        Err(anyhow::anyhow!(traceback))
                                                    }
                                                } else {
                                                    traceback.push_str("Failed to read image from cache, attempting to load from api\n");
                                                    let image = reqwest::blocking::get(movecard.card_image.small.url.as_str());
                                                    if let Ok(image) = image {
                                                        let image_bytes = image.bytes();
                                                        if let Ok(image_bytes) = image_bytes {
                                                            let raw_image_bytes = image_bytes.to_vec();
                                                            let res = std::fs::write(format!("./cache/small/{}.cache", id), &raw_image_bytes);
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
                                            }));
                                            if let Ok(i) = i {
                                                card.card_image.small.promise_index = Some(i);
                                            } else {
                                                eprintln!("Failed to create promise for small image: {}", id);
                                            }
                                        } else {
                                            // eprintln!("Failed to get index for image promise");
                                        }
                                    }
                                    if let Some(input_position) = input_position {
                                        if image_rect.contains(input_position) {
                                            card_to_draw = Some((card.clone(), image_rect));
                                        }
                                    }
                                }
                                if let Some((card, image_rect)) = card_to_draw {
                                    let card = self.cards.iter_mut().find(|c| c.id == card.id).unwrap();
                                    if let Some(input_position) = input_position {
                                        if let Some(image) = card.card_image.large.image {
                                            let min_x = input_position.x;
                                            let ui_rect = ui.clip_rect();
                                            let max_x = ui_rect.max.y * ASPECT_RATIO;
                                            let bigimage_rect = if max_x < ui_rect.max.x - min_x {
                                                let min_y = ui_rect.min.y;
                                                let max_y = ui_rect.max.y;
                                                let max_x = input_position.x + ((max_y - min_y) * ASPECT_RATIO);
                                                egui::Rect::from_min_max(egui::Pos2::new(min_x, min_y), egui::Pos2::new(max_x, max_y))
                                            } else if input_position.y < ui_rect.center().y {
                                                let min_y = ui_rect.min.y;
                                                let max_x = ui_rect.max.x;
                                                let max_y = min_y + (max_x - min_x) / ASPECT_RATIO;
                                                egui::Rect::from_min_max(egui::Pos2::new(min_x, min_y), egui::Pos2::new(max_x, max_y))
                                            } else {
                                                let max_y = ui_rect.max.y;
                                                let max_x = ui_rect.max.x;
                                                let min_y = max_y - (max_x - min_x) / ASPECT_RATIO;
                                                egui::Rect::from_min_max(egui::Pos2::new(min_x, min_y), egui::Pos2::new(max_x, max_y))
                                            };
                                            let mut mesh = egui::Mesh::with_texture(image);
                                            mesh.add_rect_with_uv(bigimage_rect, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1., 1.)), egui::Color32::WHITE);
                                            ui.painter().add(egui::Shape::Mesh(mesh));
                                        } else if let Some(_promise_index) = card.card_image.large.promise_index {
                                            ui.painter().text(
                                                image_rect.center(),
                                                egui::Align2::CENTER_CENTER,
                                                "Loading...",
                                                egui::FontId::default(),
                                                egui::Color32::from_rgb(0, 0, 0),
                                            );
                                            self.request_repaint = true;
                                            // let im = self.image_promises.get_ref(promise_index);
                                            // if let Some(im) = im {
                                            //     if im.poll().is_ready() {
                                            //         let im = self.image_promises.try_take(promise_index);
                                            //         if let Some(im) = im {
                                            //             let im = im.block_and_take();
                                            //             if let Ok(im) = im {
                                            //                 card.card_image.large.image = Some(im.texture_id(ctx));
                                            //                 card.card_image.large.promise_index = None;
                                            //                 self.cached_images.push(im);
                                            //             } else {
                                            //                 eprintln!("Image promise exists but was not fulfilled");
                                            //             }
                                            //         } else {
                                            //             eprintln!("Image promise could not be taken");
                                            //         }
                                            //     } else {
                                            //         //eprintln!("Image promise is not ready");
                                            //     }
                                            // } else {
                                            //     eprintln!("Image promise does not exist");
                                            //     card.card_image.large.promise_index = None;
                                            // }
                                        } else {
                                            let movecard = card.clone();
                                            let id = card.id;
                                            let api_override = self.api_override;
                                            if self.image_promises.get_index().is_some() {
                                                let i = self.image_promises.try_add(Promise::spawn_thread(format!("large:{}", card.id), move || {
                                                    let mut traceback = String::new();
                                                    let image_bytes: Result<Vec<u8>, anyhow::Error> = if api_override {
                                                        Err(anyhow::Error::msg("API override"))
                                                    } else {
                                                        let r = std::fs::read(format!("./cache/large/{}.cache", id));
                                                        if let Ok(data) = r {
                                                            Ok(data)
                                                        } else {
                                                            Err(anyhow::Error::msg("Failed to read image"))
                                                        }
                                                    };
                                                    if let Ok(image_bytes) = image_bytes {
                                                        let image = RetainedImage::from_image_bytes(format!("large:{}", id), &image_bytes[..]);
                                                        if let Ok(image) = image {
                                                            Ok(image)
                                                        } else {
                                                            let res = std::fs::remove_file(format!("./cache/large/{}.cache", id));
                                                            if res.is_ok() {
                                                                traceback.push_str("Failed to load image bytes\n");
                                                            } else {
                                                                traceback.push_str("Failed to load image bytes\nFailed to delete corrupted image from cache\n");
                                                            }
                                                            Err(anyhow::anyhow!(traceback))
                                                        }
                                                    } else {
                                                        traceback.push_str("Failed to read image from cache, attempting to load from api\n");
                                                        let image = reqwest::blocking::get(movecard.card_image.large.url.as_str());
                                                        if let Ok(image) = image {
                                                            let image_bytes = image.bytes();
                                                            if let Ok(image_bytes) = image_bytes {
                                                                let raw_image_bytes = image_bytes.to_vec();
                                                                let res = std::fs::write(format!("./cache/large/{}.cache", id), &raw_image_bytes);
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
                                                }));
                                                if let Ok(i) = i {
                                                    card.card_image.large.promise_index = Some(i);
                                                } else {
                                                    eprintln!("Failed to create promise for large image: {}", id);
                                                }
                                            } else {
                                                // eprintln!("Failed to get index for image promise");
                                            }
                                        }
                                    }
                                }
                            });
                        }
                    }
                    ListDisplayMode::ImageOnly => {
                        ui.label("Image Only");
                        if ui.button("Card").clicked() {
                            self.list_display_mode = ListDisplayMode::Card;
                        }
                        ui.separator();
                        if let Some(search_results) = self.search_results.as_mut() {
                            // determine how many columns we can fit based on the width of the window and the CARD_WIDTH

                            let width = ui.available_rect_before_wrap().width();
                            // ui.painter().rect_stroke(ui.available_rect_before_wrap(), 1.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(0, 0, 0)));
                            let split = width / (CARD_WIDTH + CARD_MARGIN);
                            let columns = split.floor() as usize;
                            let scaling = (split - (columns as f32 * 0.0885)) / columns as f32;

                            let mut card_to_draw = None;
                            let input_position = ui.input().pointer.hover_pos();
                            ScrollArea::vertical().show_rows(
                                ui,
                                (CARD_HEIGHT + CARD_MARGIN) * scaling,
                                (search_results.len() as f32 / columns as f32).ceil() as usize,
                                |ui, range| {
                                    // this will be a grid of cards, the calculated number of columns wide and the calculated number of rows high

                                    for row in range {
                                        ui.horizontal(|ui| {
                                            for column in 0..columns {
                                                let index = row * columns + column;
                                                if index < search_results.len() {
                                                    let card = self.cards[search_results[index].0].as_mut();
                                                    let (_id, rect) = ui.allocate_space(egui::Vec2::new((CARD_WIDTH + CARD_MARGIN) * scaling, (CARD_HEIGHT + CARD_MARGIN) * scaling));
                                                    ui.painter()
                                                        .rect(rect, CARD_ROUNDING, egui::Color32::from_rgb(54, 54, 54), egui::Stroke::new(1., egui::Color32::from_rgb(64, 64, 64)));
                                                    if let Some(image) = card.card_image.small.image {
                                                        let mut mesh = egui::Mesh::with_texture(image);
                                                        mesh.add_rect_with_uv(rect, egui::Rect::from_min_max(egui::Pos2::new(0., 0.), egui::Pos2::new(1., 1.)), egui::Color32::WHITE);
                                                        ui.painter().add(egui::Shape::Mesh(mesh));
                                                    } else if let Some(_promise_index) = card.card_image.small.promise_index {
                                                        ui.painter()
                                                            .text(rect.center(), egui::Align2::CENTER_CENTER, "Loading...", egui::FontId::default(), egui::Color32::from_rgb(0, 0, 0));
                                                        self.request_repaint = true;
                                                        // let im = self.image_promises.get_ref(promise_index);
                                                        // if let Some(im) = im {
                                                        //     if im.poll().is_ready() {
                                                        //         let im = self.image_promises.try_take(promise_index);
                                                        //         if let Some(im) = im {
                                                        //             let im = im.block_and_take();
                                                        //             if let Ok(im) = im {
                                                        //                 card.card_image.small.image = Some(im.texture_id(ctx));
                                                        //                 card.card_image.small.promise_index = None;
                                                        //                 self.cached_images.push(im);
                                                        //             } else {
                                                        //                 eprintln!("Image promise exists but was not fulfilled");
                                                        //             }
                                                        //         } else {
                                                        //             eprintln!("Image promise could not be taken");
                                                        //         }
                                                        //     } else {
                                                        //         //eprintln!("Image promise is not ready");
                                                        //     }
                                                        // } else {
                                                        //     eprintln!("Image promise does not exist");
                                                        //     card.card_image.small.promise_index = None;
                                                        // }
                                                    } else {
                                                        let movecard = card.clone();
                                                        let id = card.id;
                                                        let api_override = self.api_override;
                                                        if self.image_promises.get_index().is_some() {
                                                            let i = self.image_promises.try_add(Promise::spawn_thread(format!("small:{}", card.id), move || {
                                                                let mut traceback = String::new();
                                                                let image_bytes: Result<Vec<u8>, anyhow::Error> = if api_override {
                                                                    Err(anyhow::Error::msg("API override"))
                                                                } else {
                                                                    let r = std::fs::read(format!("./cache/small/{}.cache", id));
                                                                    if let Ok(data) = r {
                                                                        Ok(data)
                                                                    } else {
                                                                        Err(anyhow::Error::msg("Failed to read image"))
                                                                    }
                                                                };
                                                                if let Ok(image_bytes) = image_bytes {
                                                                    let image = RetainedImage::from_image_bytes(format!("small:{}", id), &image_bytes[..]);
                                                                    if let Ok(image) = image {
                                                                        Ok(image)
                                                                    } else {
                                                                        let res = std::fs::remove_file(format!("./cache/small/{}.cache", id));
                                                                        if res.is_ok() {
                                                                            traceback.push_str("Failed to load image bytes\n");
                                                                        } else {
                                                                            traceback.push_str("Failed to load image bytes\nFailed to delete corrupted image from cache\n");
                                                                        }
                                                                        Err(anyhow::anyhow!(traceback))
                                                                    }
                                                                } else {
                                                                    traceback.push_str("Failed to read image from cache, attempting to load from api\n");
                                                                    let image = reqwest::blocking::get(movecard.card_image.small.url.as_str());
                                                                    if let Ok(image) = image {
                                                                        let image_bytes = image.bytes();
                                                                        if let Ok(image_bytes) = image_bytes {
                                                                            let raw_image_bytes = image_bytes.to_vec();
                                                                            let res = std::fs::write(format!("./cache/small/{}.cache", id), &raw_image_bytes);
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
                                                            }));
                                                            if let Ok(i) = i {
                                                                card.card_image.small.promise_index = Some(i);
                                                            } else {
                                                                eprintln!("Failed to create promise for small image: {}", id);
                                                            }
                                                        } else {
                                                            // eprintln!("Failed to get index for image promise");
                                                        }
                                                    }
                                                    if let Some(input_position) = input_position {
                                                        if rect.contains(input_position) {
                                                            card_to_draw = Some((card.clone(), rect));
                                                        }
                                                    }
                                                }
                                            }
                                        });
                                    }
                                    if let Some((card, image_rect)) = card_to_draw {
                                        let card = self.cards.iter_mut().find(|c| c.id == card.id).unwrap();
                                        if let Some(input_position) = input_position {
                                            if let Some(image) = card.card_image.large.image {
                                                let bigimage_rect;
                                                if input_position.x < ui.available_width() / 2. {
                                                    let min_x = input_position.x;
                                                    let ui_rect = ui.clip_rect();
                                                    let max_x = ui_rect.max.y * ASPECT_RATIO;
                                                    bigimage_rect = if max_x < ui_rect.max.x - min_x {
                                                        let min_y = ui_rect.min.y;
                                                        let max_y = ui_rect.max.y;
                                                        let max_x = input_position.x + ((max_y - min_y) * ASPECT_RATIO);
                                                        egui::Rect::from_min_max(egui::Pos2::new(min_x, min_y), egui::Pos2::new(max_x, max_y))
                                                    } else if input_position.y < ui_rect.center().y {
                                                        let min_y = ui_rect.min.y;
                                                        let max_x = ui_rect.max.x;
                                                        let max_y = min_y + (max_x - min_x) / ASPECT_RATIO;
                                                        egui::Rect::from_min_max(egui::Pos2::new(min_x, min_y), egui::Pos2::new(max_x, max_y))
                                                    } else {
                                                        let max_y = ui_rect.max.y;
                                                        let max_x = ui_rect.max.x;
                                                        let min_y = max_y - (max_x - min_x) / ASPECT_RATIO;
                                                        egui::Rect::from_min_max(egui::Pos2::new(min_x, min_y), egui::Pos2::new(max_x, max_y))
                                                    };
                                                } else {
                                                    let max_x = input_position.x;
                                                    let ui_rect = ui.clip_rect();
                                                    let min_x = ui_rect.min.y * ASPECT_RATIO;
                                                    bigimage_rect = if min_x > ui_rect.min.x - max_x {
                                                        let min_y = ui_rect.min.y;
                                                        let max_y = ui_rect.max.y;
                                                        let min_x = input_position.x - ((max_y - min_y) * ASPECT_RATIO);
                                                        egui::Rect::from_min_max(egui::Pos2::new(min_x, min_y), egui::Pos2::new(max_x, max_y))
                                                    } else if input_position.y < ui_rect.center().y {
                                                        let min_y = ui_rect.min.y;
                                                        let min_x = ui_rect.min.x;
                                                        let max_y = min_y + (max_x - min_x) / ASPECT_RATIO;
                                                        egui::Rect::from_min_max(egui::Pos2::new(min_x, min_y), egui::Pos2::new(max_x, max_y))
                                                    } else {
                                                        let max_y = ui_rect.max.y;
                                                        let min_x = ui_rect.min.x;
                                                        let min_y = max_y - (max_x - min_x) / ASPECT_RATIO;
                                                        egui::Rect::from_min_max(egui::Pos2::new(min_x, min_y), egui::Pos2::new(max_x, max_y))
                                                    };
                                                }
                                                let mut mesh = egui::Mesh::with_texture(image);
                                                mesh.add_rect_with_uv(bigimage_rect, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1., 1.)), egui::Color32::WHITE);
                                                ui.painter().add(egui::Shape::Mesh(mesh));
                                            } else if let Some(_promise_index) = card.card_image.large.promise_index {
                                                ui.painter().text(
                                                    image_rect.center(),
                                                    egui::Align2::CENTER_CENTER,
                                                    "Loading...",
                                                    egui::FontId::default(),
                                                    egui::Color32::from_rgb(0, 0, 0),
                                                );
                                                self.request_repaint = true;
                                                // let im = self.image_promises.get_ref(promise_index);
                                                // if let Some(im) = im {
                                                //     if im.poll().is_ready() {
                                                //         let im = self.image_promises.try_take(promise_index);
                                                //         if let Some(im) = im {
                                                //             let im = im.block_and_take();
                                                //             if let Ok(im) = im {
                                                //                 card.card_image.large.image = Some(im.texture_id(ctx));
                                                //                 card.card_image.large.promise_index = None;
                                                //                 self.cached_images.push(im);
                                                //             } else {
                                                //                 eprintln!("Image promise exists but was not fulfilled");
                                                //             }
                                                //         } else {
                                                //             eprintln!("Image promise could not be taken");
                                                //         }
                                                //     } else {
                                                //         //eprintln!("Image promise is not ready");
                                                //     }
                                                // } else {
                                                //     eprintln!("Image promise does not exist");
                                                //     card.card_image.large.promise_index = None;
                                                // }
                                            } else {
                                                let movecard = card.clone();
                                                let id = card.id;
                                                let api_override = self.api_override;
                                                if self.image_promises.get_index().is_some() {
                                                    let i = self.image_promises.try_add(Promise::spawn_thread(format!("large:{}", card.id), move || {
                                                        let mut traceback = String::new();
                                                        let image_bytes: Result<Vec<u8>, anyhow::Error> = if api_override {
                                                            Err(anyhow::Error::msg("API override"))
                                                        } else {
                                                            let r = std::fs::read(format!("./cache/large/{}.cache", id));
                                                            if let Ok(data) = r {
                                                                Ok(data)
                                                            } else {
                                                                Err(anyhow::Error::msg("Failed to read image"))
                                                            }
                                                        };
                                                        if let Ok(image_bytes) = image_bytes {
                                                            let image = RetainedImage::from_image_bytes(format!("large:{}", id), &image_bytes[..]);
                                                            if let Ok(image) = image {
                                                                Ok(image)
                                                            } else {
                                                                let res = std::fs::remove_file(format!("./cache/large/{}.cache", id));
                                                                if res.is_ok() {
                                                                    traceback.push_str("Failed to load image bytes\n");
                                                                } else {
                                                                    traceback.push_str("Failed to load image bytes\nFailed to delete corrupted image from cache\n");
                                                                }
                                                                Err(anyhow::anyhow!(traceback))
                                                            }
                                                        } else {
                                                            traceback.push_str("Failed to read image from cache, attempting to load from api\n");
                                                            let image = reqwest::blocking::get(movecard.card_image.large.url.as_str());
                                                            if let Ok(image) = image {
                                                                let image_bytes = image.bytes();
                                                                if let Ok(image_bytes) = image_bytes {
                                                                    let raw_image_bytes = image_bytes.to_vec();
                                                                    let res = std::fs::write(format!("./cache/large/{}.cache", id), &raw_image_bytes);
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
                                                    }));
                                                    if let Ok(i) = i {
                                                        card.card_image.large.promise_index = Some(i);
                                                    } else {
                                                        eprintln!("Failed to create promise for large image: {}", id);
                                                    }
                                                } else {
                                                    // eprintln!("Failed to get index for image promise");
                                                }
                                            }
                                        }
                                    }
                                },
                            );
                        }
                    }
                }
            }
            let mut did = false;
            for card in self.cards.iter_mut() {
                if let Some(promise_index) = card.card_image.small.promise_index {
                    did = true;
                    let im = self.image_promises.get_ref(promise_index);
                    if let Some(im) = im {
                        if im.poll().is_ready() {
                            let im = self.image_promises.try_take(promise_index);
                            if let Some(im) = im {
                                let im = im.block_and_take();
                                if let Ok(im) = im {
                                    card.card_image.small.image = Some(im.texture_id(ctx));
                                    card.card_image.small.promise_index = None;
                                    self.cached_images.push(im);
                                } else {
                                    eprintln!("Image promise exists but was not fulfilled");
                                }
                            } else {
                                eprintln!("Image promise could not be taken");
                            }
                        } else {
                            // eprintln!("Image promise is not ready");
                        }
                    } else {
                        eprintln!("Image promise does not exist");
                        card.card_image.small.promise_index = None;
                    }
                }
                if let Some(promise_index) = card.card_image.large.promise_index {
                    did = true;
                    let im = self.image_promises.get_ref(promise_index);
                    if let Some(im) = im {
                        if im.poll().is_ready() {
                            let im = self.image_promises.try_take(promise_index);
                            if let Some(im) = im {
                                let im = im.block_and_take();
                                if let Ok(im) = im {
                                    card.card_image.large.image = Some(im.texture_id(ctx));
                                    card.card_image.large.promise_index = None;
                                    self.cached_images.push(im);
                                } else {
                                    eprintln!("Image promise exists but was not fulfilled");
                                }
                            } else {
                                eprintln!("Image promise could not be taken");
                            }
                        } else {
                            //eprintln!("Image promise is not ready");
                        }
                    } else {
                        eprintln!("Image promise does not exist");
                        card.card_image.large.promise_index = None;
                    }
                }
            }
            if !did {
                // self.image_promises.clear();
            }
        });
        if self.request_repaint {
            self.request_repaint = false;
            ctx.request_repaint();
            // println!("Repaint requested");
        }
    }
}
