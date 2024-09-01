use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub komga_url: String,
    pub komga_username: String,
    pub komga_password: String,
    pub libraries: Vec<String>,
    pub bgm_key: String,
}

impl Config {
    pub fn parse() -> Config {
        let config = match std::fs::read_to_string("config.json") {
            Ok(config) => config,
            Err(e) => {
                println!("{}", e);
                std::process::exit(1);
            }
        };

        match serde_json::from_str::<Config>(&config){
            Ok(config) => config,
            Err(e) => {
                println!("{}", e);
                std::process::exit(1);
            }
        }
    }
}
