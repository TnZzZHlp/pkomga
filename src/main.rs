mod bgm;
mod config;
mod komga;

use bgm::Bgm;
use config::Config;
use komga::Komga;

use indicatif::{MultiProgress, ProgressBar, ProgressState, ProgressStyle};
use lazy_static::lazy_static;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;
use std::{fmt, sync::Arc, time::Duration};
use tokio::task::JoinSet;

lazy_static! {
    static ref STY: ProgressStyle = ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({msg})",
    )
    .unwrap()
    .with_key("eta", |state: &ProgressState, w: &mut dyn fmt::Write| {
        write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap()
    })
    .progress_chars("#>-");
}

#[tokio::main]
async fn main() {
    // 初始化日志
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("初始化日志失败");

    // 解析配置
    let config = Config::parse();

    // 初始化Komga
    let komga = Arc::new(Komga::new(&config));

    // 初始化Bangumi
    let bgm = Arc::new(Bgm::new(&config));

    // 初始化进度条
    let pb = Arc::new(MultiProgress::new());

    insert_bgm(&config, komga.clone(), bgm.clone(), pb.clone()).await;
    parse_metadata(&config, komga, bgm, pb.clone()).await;
}

async fn insert_bgm(config: &Config, komga: Arc<Komga>, bgm: Arc<Bgm>, pb: Arc<MultiProgress>) {
    // Get all libraries
    let libraries = komga.get_all_libraries().await;

    // Get all series
    let series = if config.libraries.is_empty() {
        komga.get_all_series().await
    } else {
        let library_ids: Vec<&str> = libraries
            .iter()
            .filter(|library| config.libraries.contains(&library.name))
            .map(|library| library.id.as_str())
            .collect();
        komga.get_series_by_library(library_ids).await
    };

    // Progress bar
    let insert_pb = pb.add(ProgressBar::new(series.len() as u64));
    insert_pb.set_style(STY.clone());
    insert_pb.set_message("Inserting Bangumi Url");
    insert_pb.enable_steady_tick(Duration::from_millis(100));

    //  Insert Bgmid into series
    let mut tasks = JoinSet::new();

    let limit = Arc::new(tokio::sync::Semaphore::new(2));

    for s in series.clone() {
        let komga = komga.clone();
        let limit = limit.clone();
        let bgm = bgm.clone();
        let insert_pb = insert_pb.clone();

        // 判断是否已经存在Bangumi链接
        if s.metadata.links.iter().any(|link| link.label == "Bangumi") {
            insert_pb.inc(1);
            continue;
        }

        // 判断是否已经挂削过
        if s.metadata.tags.iter().any(|tag| tag == "已挂削") {
            insert_pb.inc(1);
            continue;
        }

        tasks.spawn(async move {
            let _permit = limit.acquire().await.unwrap();
            insert_pb.set_message(format!("Inserting Bangumi Url into {}", s.name));
            let bgmid = match bgm.search_subject(&s.name).await {
                Ok(bgmid) => bgmid,
                Err(_) => {
                    insert_pb.inc(1);
                    return;
                }
            };
            komga.insert_bgmurl(&s.id, &bgmid).await;
            insert_pb.inc(1);
        });
    }

    tasks.join_all().await;
    insert_pb.finish_with_message("Insert Bangumi Url Done");
}

async fn parse_metadata(config: &Config, komga: Arc<Komga>, bgm: Arc<Bgm>, pb: Arc<MultiProgress>) {
    // 获取所有库
    let libraries = komga.get_all_libraries().await;

    // 根据库获取所有系列
    let series = if config.libraries.is_empty() {
        komga.get_all_series().await
    } else {
        let library_ids: Vec<&str> = libraries
            .iter()
            .filter(|library| config.libraries.contains(&library.name))
            .map(|library| library.id.as_str())
            .collect();
        komga.get_series_by_library(library_ids).await
    };

    let metadata_pb = pb.add(ProgressBar::new(series.len() as u64));
    metadata_pb.set_style(STY.clone());
    metadata_pb.set_message("Parsing Metadata");
    metadata_pb.enable_steady_tick(Duration::from_millis(100));

    let limit = Arc::new(tokio::sync::Semaphore::new(3));

    // 解析元数据
    let mut tasks = JoinSet::new();
    for s in series {
        let komga = komga.clone();
        let metadata_pb = metadata_pb.clone();
        let pb = pb.clone();
        let limit = limit.clone();
        let bgm = bgm.clone();

        // 判断是否已经挂削过
        if s.clone().metadata.tags.iter().any(|tag| tag == "已挂削") {
            metadata_pb.inc(1);
            continue;
        }

        // 判断是否已经存在Bangumi链接
        if !s
            .clone()
            .metadata
            .links
            .iter()
            .any(|link| link.label == "Bangumi")
        {
            metadata_pb.inc(1);
            continue;
        }

        let info_pb = pb.add(ProgressBar::new(1));
        info_pb.set_style(
            ProgressStyle::with_template("{spinner:.green} {wide_msg}")
                .unwrap()
                .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ "),
        );

        // 根据Bangumi链接获取元数据
        tasks.spawn(async move {
            let _permit = limit.acquire().await.unwrap();

            info_pb.set_message(format!("{}", s.name));
            info_pb.enable_steady_tick(Duration::from_millis(100));

            // 通过Bangumi链接获取Bangumi ID
            let links = s.clone().metadata.links.clone();
            let bgmid = match s
                .clone()
                .metadata
                .links
                .into_iter()
                .find(|i| i.label == "Bangumi")
            {
                Some(link) => match link.url.split("/").last() {
                    Some(id) => id.to_string(),
                    None => {
                        info_pb.finish_and_clear();
                        metadata_pb.inc(1);
                        return;
                    }
                },
                None => {
                    info_pb.finish_and_clear();
                    metadata_pb.inc(1);
                    return;
                }
            };

            let (metadata, img) = match bgm.get_subject(&bgmid).await {
                Ok((mut metadata, img)) => {
                    metadata.links.extend(links);
                    (metadata, img)
                }
                Err(_) => {
                    info_pb.finish_and_clear();
                    metadata_pb.inc(1);
                    return;
                }
            };

            // 更新元数据
            komga.update_metadata(&s.id, metadata).await;
            komga.update_cover(&s.id, img).await;
            info_pb.finish_and_clear();
            metadata_pb.inc(1);
        });
    }

    tasks.join_all().await;
    metadata_pb.finish_with_message("Parse Metadata Done");
}
