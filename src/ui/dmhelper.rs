use chrono::{DateTime, Duration, Utc};
use egui::{vec2, CentralPanel, ColorImage, TopBottomPanel};
use image::{io::Reader, DynamicImage};
use reqwest::Url;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use std::{
    collections::HashMap,
    error::Error,
    io::{self, Cursor},
};

#[derive(Deserialize, Debug)]
struct ApiResponse {
    #[serde(deserialize_with = "deserialize_ean")]
    gtin: String,
    title: Title,
    price: Price,
    #[serde(deserialize_with = "deserialize_image")]
    images: Vec<Image>,
}

#[derive(Deserialize, Debug)]
struct Title {
    headline: String,
}

#[derive(Deserialize, Debug)]
struct Image {
    src: String,
}

#[derive(Deserialize, Debug)]
struct Price {
    price: String,
}

#[derive(Debug, Clone)]
struct Product {
    ean: String,
    name: String,
    price: f32,
    quantity: i32,
    image: Option<ColorImage>,
}

impl TryFrom<Value> for ApiResponse {
    type Error = &'static str;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match &value {
            Value::Object(map) => {
                if map.is_empty() {
                    Err("API response is an empty object")
                } else {
                    serde_json::from_value(value).map_err(|_| "Nie znaleziono produktu")
                }
            }
            _ => Err("Unexpected API response type"),
        }
    }
}

fn deserialize_ean<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Value = Deserialize::deserialize(deserializer)?;
    match value {
        Value::Number(num) => Ok(num.to_string()),
        _ => Err(serde::de::Error::custom("EAN nie jest liczbą")),
    }
}

fn deserialize_image<'de, D>(deserializer: D) -> Result<Vec<Image>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Value = Deserialize::deserialize(deserializer)?;
    match value {
        Value::Array(map) => {
            return Ok(map
                .iter()
                .map(|item| Image {
                    src: item["src"].to_string().replace("\"", ""),
                })
                .collect());
        }
        _ => Err(serde::de::Error::custom("Unexpected image field type")),
    }
}

fn download_image(input_url: &str) -> Result<DynamicImage, Box<dyn Error>> {
    let url = match Url::parse(input_url) {
        Ok(url) => url,
        Err(e) => {
            return Err(Box::new(e));
        }
    };
    let response = match reqwest::blocking::get(url) {
        Ok(response) => response,
        Err(e) => {
            return Err(Box::new(e));
        }
    };
    if !response.status().is_success() {
        return Err(format!("Failed to download image: {}", response.status()).into());
    }

    let bytes = response.bytes()?;
    let cursor = Cursor::new(bytes);
    let image = Reader::new(cursor).with_guessed_format()?.decode()?;

    Ok(image)
}

fn image_to_color_image(image: DynamicImage) -> ColorImage {
    let rgba = image.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    ColorImage::from_rgba_unmultiplied(size, rgba.as_flat_samples().as_slice())
}

impl From<ApiResponse> for Product {
    fn from(api_response: ApiResponse) -> Self {
        let price: f32 = api_response.price.price.parse().unwrap_or(0.0);
        let image_url = api_response.images[0].src.to_string();
        let url = image_url.as_str();
        let image = match download_image(url) {
            Ok(img) => Some(image_to_color_image(img)),
            Err(_e) => None,
        };

        Product {
            ean: api_response.gtin.to_string(),
            name: api_response.title.headline,
            price,
            quantity: 0,
            image,
        }
    }
}

struct CachedItem {
    product: Product,
    expires_at: DateTime<Utc>,
}

pub struct DMHelper {
    euro_exchange_rate: f32,
    cached_items: HashMap<String, CachedItem>,
    ean: String,
    cart: Vec<Product>,
    product: Option<Product>,
}

impl DMHelper {
    pub fn new() -> Self {
        return Self {
            euro_exchange_rate: 0.0,
            cached_items: HashMap::new(),
            ean: String::new(),
            cart: Vec::new(),
            product: None,
        };
    }

    fn fetch_product_info(
        ean: &str,
        cache: &mut HashMap<String, CachedItem>,
    ) -> Result<Product, Box<dyn std::error::Error>> {
        let now = Utc::now();
        if let Some(cached_item) = cache.get(ean) {
            if cached_item.expires_at > now {
                return Ok(cached_item.product.clone());
            }
        }
        let url = format!(
            "https://products.dm.de/product/DE/products/detail/gtin/{}",
            ean
        );
        let resp: Value = reqwest::blocking::get(&url)?.json()?;

        let api_response: Result<ApiResponse, _> = ApiResponse::try_from(resp);
        match api_response {
            Ok(api_response) => {
                let product: Product = Product::from(api_response);
                cache.insert(
                    ean.to_string(),
                    CachedItem {
                        product: product.clone(),
                        expires_at: now + Duration::minutes(30),
                    },
                );
                Ok(product)
            }
            Err(e) => Err(Box::new(io::Error::new(io::ErrorKind::Other, e))),
        }
    }
}

impl eframe::App for DMHelper {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.set_height(25.0);
                ui.horizontal(|ui| {
                    ui.label("DMHelper");
                });
            });
            ui.horizontal(|ui| {
                ui.label("Exchange Rate:");
                ui.add(egui::DragValue::new(&mut self.euro_exchange_rate).speed(0.01));
            });
        });
        CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.text_edit_singleline(&mut self.ean);
                    if ui.button("Pobierz informacje o produkcie").clicked() {
                        match DMHelper::fetch_product_info(&self.ean.trim(), &mut self.cached_items)
                        {
                            Ok(product) => {
                                self.product = Some(product.clone());
                            }
                            Err(e) => {
                                ui.label(format!("Błąd: {}", e));
                            }
                        }
                    }
                    if let Some(product) = &mut self.product {
                        ui.label(format!("Znaleziono produkt {}", product.name));
                        ui.label(product.ean.clone());

                        if let Some(ref image) = product.image {
                            let texture = ctx.load_texture(
                                "product_image",
                                image.clone(),
                                egui::TextureOptions::default(),
                            );
                            ui.add(
                                egui::Image::from_texture(&texture).max_size(vec2(100.0, 200.0)),
                            );
                        } else {
                            ui.label("Failed to load image");
                        }

                        ui.horizontal(|ui| {
                            ui.label("Ilość:");
                            ui.add(egui::widgets::DragValue::new(&mut product.quantity).speed(1.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label(format!("Cena w EURO: {:.2}", product.price));
                            ui.label(format!(
                                "Cena w PLN: {:.2}",
                                product.price * self.euro_exchange_rate * product.quantity as f32
                            ));
                        });
                        if ui.button("Dodaj do koszyka").clicked() {
                            if product.quantity == 0 {
                                return;
                            }
                            if self.cart.iter().any(|item| item.ean == product.ean) {
                                let index = self
                                    .cart
                                    .iter()
                                    .position(|item| item.ean == product.ean)
                                    .unwrap();
                                self.cart[index].quantity += product.quantity;
                            } else {
                                self.cart.push(product.clone());
                            }
                            self.product = None;
                        };
                    }
                    ui.add_space(300.0);
                });
                ui.separator();
                ui.vertical(|ui| {
                    let total_price: f32 = self
                        .cart
                        .iter()
                        .map(|item| item.price * item.quantity as f32)
                        .sum();
                    egui::ScrollArea::vertical()
                        .max_height(ui.available_height() - 100.0)
                        .max_width(ui.available_width())
                        .auto_shrink(false)
                        .show(ui, |ui| {
                            for item in &self.cart {
                                ui.horizontal(|ui| {
                                    ui.label(item.name.to_string());
                                    ui.label(item.quantity.to_string());
                                    ui.label(format!("€{:.2}", item.price));
                                });
                                ui.separator();
                            }
                        });
                    ui.label(format!("Kurs euro: {}", self.euro_exchange_rate));
                    ui.label(format!(
                        "\n\nSuma: €{:.2}, suma: {:.2}PLN",
                        total_price,
                        total_price * self.euro_exchange_rate
                    ))
                });
            })
        });
        ctx.request_repaint();
    }
}
