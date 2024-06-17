use std::io;
use reqwest;
use serde::{de::Error, Deserialize, Deserializer}; 
use serde_json::value::Number;


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

#[derive(Debug)]
struct Product {
    ean: String,
    name: String,
    price: f32,
    quantity: i32,
}

fn deserialize_ean<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    // Deserialize the value as a `Value` and handle the conversion
    let value: Value = Deserialize::deserialize(deserializer)?;

    match value {
        Value::Number(num) => Ok(num.to_string()),
        _ => Err(serde::de::Error::custom("Expected a number")),
    }
}

impl Product {
    fn from_api_response(api_response: ApiResponse, quantity: i32) -> Self {
        let price: f32 = api_response.price.price.parse().unwrap_or(0.0);
        Product {
            ean: api_response.gtin.to_string(),
            name: api_response.title.headline,
            price,
            quantity,
        }
    }
}


fn fetch_product_info(ean: &str, quantity: i32) -> Result<Product, reqwest::Error> {
    let url = format!("https://products.dm.de/product/DE/products/detail/gtin/{}", ean);
    let resp: ApiResponse = reqwest::blocking::get(&url)?.json()?;

    Ok(Product::from_api_response(resp, quantity))
}


fn main() {
    loop {
        let mut ean: String = String::new();
        println!("Podaj EAN produktu (w celu zakończenia wpisz end):");
        io::stdin().read_line(&mut ean).unwrap();
        if ean.to_lowercase().trim() == "end" {
            break;
        }
        let product: Product = fetch_product_info(&ean.trim().to_lowercase(), 10).unwrap();

        println!("{:?}", product);
    }
    
}
