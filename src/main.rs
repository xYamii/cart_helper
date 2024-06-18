use std::io;
use std::convert::TryFrom;
use reqwest;
use serde::{Deserialize, Deserializer}; 
use serde_json::Value;
use chrono::{DateTime, Utc, Duration};
use structopt::StructOpt;
use std::collections::HashMap;

#[derive(Deserialize, Debug)]
struct ApiResponse {
    #[serde(deserialize_with = "deserialize_ean")]
    gtin: String,
    title: Title,
    price: Price,
}

#[derive(Deserialize, Debug)]
struct Title {
    headline: String,
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
}

struct CachedItem {
    product: Product,
    expires_at: DateTime<Utc>,
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
            },
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
impl From<ApiResponse> for Product {
    fn from(api_response: ApiResponse) -> Self {
        let price: f32 = api_response.price.price.parse().unwrap_or(0.0);
        Product {
            ean: api_response.gtin.to_string(),
            name: api_response.title.headline,
            price,
            quantity: 0,
        }
    }
}

fn fetch_product_info(ean: &str, cache: &mut HashMap<String, CachedItem>) -> Result<Product, Box<dyn std::error::Error>> {
    let now = Utc::now();
    if let Some(cached_item) = cache.get(ean) {
        if cached_item.expires_at > now {
            println!("Pobieram z cache");
            return Ok(cached_item.product.clone());
        }
    }

    let url = format!("https://products.dm.de/product/DE/products/detail/gtin/{}", ean);
    let resp: Value = reqwest::blocking::get(&url)?.json()?;

    let api_response: Result<ApiResponse, _> = ApiResponse::try_from(resp);
    match api_response {
        Ok(api_response) => {
            let product: Product = Product::from(api_response);
            cache.insert(ean.to_string(), CachedItem {
                product: product.clone(),
                expires_at: now + Duration::minutes(30),
            });
            Ok(product)
        },
        Err(e) => {
            Err(Box::new(io::Error::new(io::ErrorKind::Other, e)))
        }
    }
}

fn main() {
    let mut cache = HashMap::new();
    let mut cart = Vec::new();
    let exchange_rate: f32 = 4.34;
    loop {
        let mut ean: String = String::new();
        let mut quantity_input: String = String::new();
        println!("Podaj EAN produktu (w celu zakończenia wpisz end):");
        io::stdin().read_line(&mut ean).unwrap();
        if ean.to_lowercase().trim() == "end" {
            break;
        }
        match fetch_product_info(&ean.trim().to_lowercase(), &mut cache) {
            Ok(mut product) => {
                println!("Znaleziono produkt {}", product.name);
                println!("Podaj ilość produktu:");
                io::stdin().read_line(&mut quantity_input).unwrap();
                let quantity: i32 = quantity_input.trim().parse().unwrap();
                product.quantity = quantity;
                cart.push(product);
            }
            Err(e) => println!("Błąd przy pobieraniu informacji o produkcie: {}", e),
        }
    }

    let total_price: f32 = cart.iter().map(|item| item.price * item.quantity as f32).sum();
    println!("Twoje produkty:");
    for item in &cart {
        println!(" - {} sztuk: {}  |  €{:.2} za sztukę", item.name, item.quantity, item.price);
    }
    println!("\nKurs euro: {}", exchange_rate);
    println!("\n\nSuma: €{:.2}, suma: {:.2}PLN", total_price, total_price * exchange_rate);
}
