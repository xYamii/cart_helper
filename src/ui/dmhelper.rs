use chrono::{DateTime, Duration, Utc};
use egui::{CentralPanel, TopBottomPanel};
use reqwest;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use std::{collections::HashMap, io};

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
    image: String,
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
                    src: item["src"].to_string(),
                })
                .collect());
        }
        _ => Err(serde::de::Error::custom("Unexpected image field type")),
    }
}

impl From<ApiResponse> for Product {
    fn from(api_response: ApiResponse) -> Self {
        let price: f32 = api_response.price.price.parse().unwrap_or(0.0);
        Product {
            ean: api_response.gtin.to_string(),
            name: api_response.title.headline,
            price,
            quantity: 0,
            image: api_response.images[0].src.to_string(),
        }
    }
}

struct CachedItem {
    product: Product,
    expires_at: DateTime<Utc>,
}

pub struct DMHelper {
    cached_items: HashMap<String, CachedItem>,
    ean: String,
    cart: Vec<Product>,
    product: Option<Product>,
}

impl DMHelper {
    pub fn new() -> Self {
        return Self {
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
                println!("Pobieram z cache");
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
        let exchange_rate: f32 = 4.34;
        TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.set_height(25.0);
                ui.horizontal(|ui| {
                    ui.label("DMHelper");
                });
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
                        //doesnt work :(
                        ui.add(egui::Image::from_uri(product.image.clone()));
                        ui.horizontal(|ui| {
                            ui.label("Ilość:");
                            ui.add(egui::widgets::DragValue::new(&mut product.quantity).speed(1.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label(format!("Cena w EURO: {:.2}", product.price));
                            ui.label(format!(
                                "Cena w PLN: {:.2}",
                                product.price * exchange_rate * product.quantity as f32
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
                    ui.label(format!("Kurs euro: {}", exchange_rate));
                    ui.label(format!(
                        "\n\nSuma: €{:.2}, suma: {:.2}PLN",
                        total_price,
                        total_price * exchange_rate
                    ))
                });
            })
        });
        ctx.request_repaint();
    }
}
