use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::Config;

#[derive(Debug, Serialize, Deserialize)]
pub struct Komga {
    pub url: String,
    pub user: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Libraries {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Series {
    pub id: String,
    pub name: String,
    #[serde(rename = "libraryId")]
    pub library_id: String,
    pub metadata: Metadata,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub status: String,
    pub summary: String,
    pub publisher: String,
    pub tags: Vec<String>,
    pub links: Vec<Link>,
    pub alternate_titles: Vec<AlternateTitle>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct AlternateTitle {
    pub label: String,
    pub title: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Link {
    pub label: String,
    pub url: String,
}

impl Metadata {
    pub fn new() -> Metadata {
        Metadata {
            status: "ONGOING".to_owned(),
            summary: "".to_owned(),
            publisher: "".to_owned(),
            tags: Vec::new(),
            links: Vec::new(),
            alternate_titles: Vec::new(),
        }
    }
}

impl Komga {
    pub fn new(config: &Config) -> Komga {
        Komga {
            url: config.komga_url.clone(),
            user: config.komga_username.clone(),
            password: config.komga_password.clone(),
        }
    }

    pub async fn get_all_libraries(&self) -> Vec<Libraries> {
        reqwest::Client::new()
            .get(&format!("{}/api/v1/libraries?size=100000", self.url))
            .basic_auth(&self.user, Some(&self.password))
            .send()
            .await
            .expect("Failed to get libraries")
            .json::<Vec<Libraries>>()
            .await
            .expect("Failed to parse libraries")
    }

    pub async fn get_all_series(&self) -> Vec<Series> {
        #[derive(Deserialize)]
        struct Response {
            content: Vec<Series>,
        }
        reqwest::Client::new()
            .get(&format!("{}/api/v1/series?size=100000", self.url))
            .basic_auth(&self.user, Some(&self.password))
            .send()
            .await
            .expect("Failed to get series")
            .json::<Response>()
            .await
            .expect("Failed to parse series")
            .content
    }

    pub async fn get_series_by_library(&self, library_ids: Vec<&str>) -> Vec<Series> {
        #[derive(Deserialize)]
        struct Response {
            content: Vec<Series>,
        }

        let mut series: Vec<Series> = Vec::new();
        for library_id in library_ids {
            series.append(
                &mut reqwest::Client::new()
                    .get(&format!(
                        "{}/api/v1/series?library_id={}&size=100000",
                        self.url, library_id
                    ))
                    .basic_auth(&self.user, Some(&self.password))
                    .send()
                    .await
                    .expect("Failed to get series")
                    .json::<Response>()
                    .await
                    .expect("Failed to parse series")
                    .content,
            );
        }
        series
    }

    pub async fn insert_bgmurl(&self, series_id: &str, bgmid: &str) {
        #[derive(Deserialize)]
        struct Res {
            metadata: Metadata,
        }

        // 判断是否已经存在Bangumi链接
        match reqwest::Client::new()
            .get(&format!("{}/api/v1/series/{}", self.url, series_id))
            .basic_auth(&self.user, Some(&self.password))
            .send()
            .await
            .expect("Failed to get series")
            .json::<Res>()
            .await
        {
            Ok(res) => {
                if res
                    .metadata
                    .links
                    .iter()
                    .any(|link| link.label == "Bangumi")
                {
                    return;
                }
            }
            Err(_) => {
                return;
            }
        };

        reqwest::Client::new()
            .patch(&format!(
                "{}/api/v1/series/{}/metadata",
                self.url, series_id
            ))
            .basic_auth(&self.user, Some(&self.password))
            .json(&json!({
                "links": [
                    {
                        "label": "Bangumi",
                        "url": "https://bgm.tv/subject/".to_owned() + bgmid
                    }
                ]
            }))
            .send()
            .await
            .expect("Failed to insert bgmid");
    }

    pub async fn update_metadata(&self, id: &str, metadata: Metadata) {
        reqwest::Client::new()
            .patch(&format!("{}/api/v1/series/{}/metadata", self.url, id))
            .basic_auth(&self.user, Some(&self.password))
            .json(&metadata)
            .send()
            .await
            .expect("Failed to update metadata");
    }

    pub async fn update_cover(&self, id: &str, img: String) {
        #[derive(Deserialize)]
        struct Img {
            id: String,
        }

        // 判断是否已经存在封面
        match reqwest::Client::new()
            .get(&format!("{}/api/v1/series/{}/thumbnails", self.url, id))
            .basic_auth(&self.user, Some(&self.password))
            .send()
            .await
        {
            Ok(res) => match res.json::<Vec<Img>>().await {
                Ok(imgs) => {
                    for img in imgs {
                        reqwest::Client::new()
                            .delete(&format!(
                                "{}/api/v1/series/{}/thumbnails/{}",
                                self.url, id, img.id
                            ))
                            .basic_auth(&self.user, Some(&self.password))
                            .send()
                            .await
                            .expect("Failed to delete cover");
                    }
                }
                Err(_) => {}
            },
            Err(_) => {}
        }

        // 下载图片
        let img = match reqwest::Client::new().get(&img).send().await {
            Ok(res) => match res.bytes().await {
                Ok(img) => img,
                Err(_) => {
                    return;
                }
            },
            Err(_) => {
                return;
            }
        };

        // 创建 multipart/form-data 表单
        let form = Form::new()
            .part(
                "file",
                match Part::bytes(img.to_vec())
                    .file_name("cover.jpg")
                    .mime_str("image/jpeg")
                {
                    Ok(part) => part,
                    Err(_) => {
                        return;
                    }
                },
            )
            .part("selected", Part::text("true"));

        // 上传图片
        reqwest::Client::new()
            .post(&format!("{}/api/v1/series/{}/thumbnails", self.url, id))
            .basic_auth(&self.user, Some(&self.password))
            .multipart(form)
            .send()
            .await
            .expect("Failed to update cover");
    }
}
