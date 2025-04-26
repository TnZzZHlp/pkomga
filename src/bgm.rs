use crate::{
    config::Config,
    komga::{AlternateTitle, Metadata},
};
use serde::{Deserialize, Serialize};

pub struct Bgm {
    api_key: String,
    client: reqwest::Client,
}

#[derive(Deserialize, Debug)]
pub struct Subject {
    images: Images,
    summary: String,
    tags: Vec<Tag>,
    infobox: Vec<Infobox>,
}

#[derive(Deserialize, Debug)]
pub struct Images {
    large: String,
}

#[derive(Deserialize, Debug)]
pub struct Infobox {
    key: String,
    value: ValueUnion,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum ValueUnion {
    String(String),
    ValueElementArray(Vec<ValueElement>),
}

#[derive(Deserialize, Debug, Clone)]
pub struct ValueElement {
    v: String,
}

#[derive(Deserialize, Debug)]
pub struct Tag {
    name: String,
}

impl Bgm {
    pub fn new(config: &Config) -> Bgm {
        Bgm {
            api_key: config.bgm_key.clone(),
            client: reqwest::Client::new(),
        }
    }

    pub async fn search_subject(&self, name: &str) -> Result<String, ()> {
        #[derive(Serialize, Deserialize, Debug)]
        struct Res {
            list: Vec<Subject>,
        }

        #[derive(Serialize, Deserialize, Debug)]
        struct Subject {
            id: i64,
            name: String,
            name_cn: String,
        }

        let res = match self
            .client
            .get(format!(
                "https://api.bgm.tv/search/subject/\"{}\"?type=1",
                name
            ))
            .header("Authorization", "Bearer ".to_owned() + &self.api_key)
            .header(
                "User-Agent",
                "TnZzZHlp/pkomga (https://github.com/TnZzZHlp/pkomga)",
            )
            .send()
            .await
        {
            Ok(res) => res,
            Err(_) => return Err(()),
        };

        let res: Res = match res.json().await {
            Ok(res) => res,
            Err(_) => return Err(()),
        };

        Ok(res.list[0].id.to_string())
    }

    pub async fn get_subject(&self, id: &str) -> Result<(Metadata, String), String> {
        let res = match self
            .client
            .get(format!("https://api.bgm.tv/v0/subjects/{}", id))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header(
                "User-Agent",
                "TnZzZHlp/pkomga (https://github.com/TnZzZHlp/pkomga)",
            )
            .header("Accept", "application/json")
            .send()
            .await
        {
            Ok(res) => res,
            Err(e) => {
                return Err(e.to_string());
            }
        };

        let res = res.text().await.unwrap();

        let json = serde_json::from_str::<Subject>(&res);

        match json {
            Ok(json) => {
                let mut metadata = Metadata::new();

                // 判断是否完结
                if json.infobox.iter().any(|i| i.key == "结束") {
                    metadata.status = "ENDED".to_owned();
                }

                // 判断总结
                if !json.summary.is_empty() {
                    metadata.summary = json.summary;
                }

                // 判断出版社
                if json.infobox.iter().any(|i| i.key == "出版社") {
                    metadata.publisher = match json.infobox.iter().find(|i| i.key == "出版社") {
                        Some(i) => match i.value.clone() {
                            ValueUnion::String(s) => s,
                            _ => "".to_owned(),
                        },
                        None => "".to_owned(),
                    };
                }

                // 判断标签
                metadata.tags = json.tags.iter().map(|t| t.name.clone()).collect();
                metadata.tags.push("已挂削".to_owned());

                // 写入副标题
                let mut alternate_titles: Vec<AlternateTitle> = Vec::new();
                for info in json.infobox.iter() {
                    alternate_titles.push(AlternateTitle {
                        label: info.key.clone(),
                        title: match info.value.clone() {
                            ValueUnion::String(s) => s,
                            ValueUnion::ValueElementArray(a) => a
                                .iter()
                                .map(|v| v.v.clone())
                                .collect::<Vec<String>>()
                                .join(" "),
                        },
                    });
                }
                metadata.alternate_titles = alternate_titles;

                Ok((metadata, json.images.large))
            }
            Err(_) => Err(res),
        }
    }
}
