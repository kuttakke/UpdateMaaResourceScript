use futures_util::StreamExt;
use serde_json::Value;
use spinoff::{spinners, Color, Spinner, Streams};
use std::cmp::min;
use std::path::PathBuf;
use std::{
    env, fs, io,
    io::{Read, Write},
};
// use console::{style, Emoji};
use anyhow::Result;
use chrono::{DateTime, Local, TimeZone, Utc};
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::File;
use zip::ZipArchive;

fn get_current_dir() -> PathBuf {
    env::current_exe()
        .expect("无法获得当前程序路径")
        .parent()
        .expect("无法获得当前程序目录")
        .to_path_buf()
}

fn is_maa_root_dir(path: &PathBuf) -> bool {
    path.join("cache").is_dir() && path.join("resource").is_dir() && path.join("MAA.exe").is_file()
}

fn wait_enter_and_exit() {
    println!("按回车键退出程序...");
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .expect("无法读取输入");
    std::process::exit(0);
}

fn get_last_update_time(path: &PathBuf) -> Option<u64> {
    if path.is_file() {
        let mut file = File::open(path).expect("无法打开上次更新时间文件");
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .expect("无法读取上次更新时间文件内容");
        let last_update_time: u64 = contents.trim().parse().expect("无法解析上次更新时间");
        Some(last_update_time)
    } else {
        None
    }
}

async fn is_github_updated_after(time: &str) -> Result<bool> {
    let url = format!(
        "https://api.github.com/repos/MaaAssistantArknights/MaaResource/commits?since={}&page=1",
        time
    );

    let client = reqwest::Client::new();

    let response = client.get(&url).header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36 Edg/119.0.0.0").send().await?;
    println!("response status: {}", response.status());
    if response.status().is_success() {
        // let text = &response.text().await?;
        // println!("response text: {}", text);
        let commits = response.json::<Value>().await?;
        if commits.as_array().expect("无法解析commits数组").len() > 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn is_need_update(path: &PathBuf) -> bool {
    let last_update_time = get_last_update_time(&path.join("last_update_time.txt"));

    if let Some(last_update_time) = last_update_time {
        // last_update_time 转为 YYYY-MM-DDTHH:MM:SSZ 格式的字符串
        let datetime: DateTime<Local> = Local.timestamp_opt(last_update_time as i64, 0).unwrap();
        let utc_datetime: DateTime<Utc> = datetime.with_timezone(&Utc);
        // 格式化日期时间为 "YYYY-MM-DDTHH:MM:SSZ" 格式
        let formatted_datetime = utc_datetime.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        // let formatted_datetime = datetime.to_rfc3339();

        // 判断是否需要更新
        // if is_github_updated_after(&formatted_datetime).unwrap() {
        //     return true;
        // } else {
        //     return false;
        // }
        println!("上次更新时间：{}", formatted_datetime);
        match is_github_updated_after(&formatted_datetime).await {
            Ok(true) => {
                return true;
            }
            Ok(false) => {
                return false;
            }
            Err(e) => {
                println!("错误信息：{}", e);
                start_maa(path);
                wait_enter_and_exit();
            }
        }
    }

    true
}

async fn updater(path: &PathBuf) -> Result<bool> {
    // 下载更新
    let url = "https://github.com/MaaAssistantArknights/MaaResource/archive/refs/heads/main.zip";
    let client = reqwest::Client::new();
    if let Ok(response) = client.get(url).send().await {
        response_handler(response, &path.join("updateResource.zip")).await?;
    } else {
        let url = "https://mirror.ghproxy.com/https://github.com/MaaAssistantArknights/MaaResource/archive/refs/heads/main.zip";
        let response = client.get(url).send().await?;

        response_handler(response, &path.join("updateResource.zip")).await?;
    }
    Ok(true)
}

async fn response_handler(response: reqwest::Response, path: &PathBuf) -> Result<bool> {
    let total_size = response.content_length().expect("无法获取下载文件大小");

    // 计算vector应该给多少长度
    let mut buf = Vec::with_capacity(total_size as usize);
    let mut progress = 0u64;

    let pb = ProgressBar::new(total_size);
    pb.set_style(ProgressStyle::default_bar()
    .template("{msg}\n{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
    ?.progress_chars("#>-"));
    pb.set_message("下载更新包");

    let mut stream = response.bytes_stream();

    while let Some(item) = stream.next().await {
        let chunk = item?;
        buf.extend_from_slice(&chunk);
        let position = min(progress + chunk.len() as u64, total_size);
        progress = position;
        pb.set_position(position);
    }
    pb.finish_and_clear();

    // 写入文件
    let mut file = File::create(path).expect("无法创建更新包文件");
    file.write_all(&buf).expect("无法写入更新包文件");

    Ok(true)
}

fn extract_update_resource(path: &PathBuf) -> Result<bool> {
    let mut zip =
        ZipArchive::new(File::open(path.join("updateResource.zip")).expect("无法打开更新包文件"))
            .expect("无法解析更新包文件");
    // 使用进度条显示解压进度
    // let total_size = zip.len();
    // 所有文件总大小
    let total_size = zip.len();
    let pb = ProgressBar::new(total_size as u64);

    pb.set_style(ProgressStyle::default_bar().progress_chars("=>-"));

    for i in 0..total_size {
        let mut entry = zip.by_index(i)?;
        // pb.set_message(format!("正在解压: {}", entry.name()));
        // println!("正在解压: {}", entry.name());
        // 创建解压路径
        if entry.name() == "MaaResource-main/" {
            continue;
        }
        let dest_path = path.join(entry.name().replace("MaaResource-main/", ""));

        // 如果是目录，则创建目录
        if entry.is_dir() {
            // 忽略已存在的目录
            if !dest_path.is_dir() {
                fs::create_dir_all(&dest_path)?;
            }
            continue;
        }

        // 如果是文件，则解压到文件
        let mut dest_file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&dest_path)?;
        io::copy(&mut entry, &mut dest_file)?;

        // 更新进度条
        pb.inc(1); // 每个文件解压后进度条增加1
    }

    // pb.set_message("解压完成");
    pb.finish_and_clear();

    Ok(true)
}

fn start_maa(path: &PathBuf) {
    let maa_path = path.join("MAA.exe");

    std::process::Command::new(maa_path)
        .spawn()
        .expect("无法启动MAA");
}

#[tokio::main]
async fn main() {
    // let current_dir = get_current_dir();
    // 创建一个旋转指示器

    let mut spinner1 = Spinner::new_with_stream(
        spinners::Dots,
        "检查当前目录...",
        Color::Yellow,
        Streams::Stderr,
    );
    let root = get_current_dir();
    // sleep(Duration::from_secs(3));
    if is_maa_root_dir(&root) {
        spinner1.stop_and_persist("✔️", "当前目录为MAA根目录");
    } else {
        spinner1.stop_and_persist("❌", "当前目录不是MAA根目录");
        start_maa(&root);
        wait_enter_and_exit();
    }
    let mut spinner2 = Spinner::new_with_stream(
        spinners::Dots,
        "检查是否需要更新...",
        Color::Yellow,
        Streams::Stderr,
    );
    if is_need_update(&root).await {
        spinner2.stop_and_persist("✔️", "需要更新");
    } else {
        spinner2.stop_and_persist("❌", "不需要更新");
        start_maa(&root);
        wait_enter_and_exit();
    }
    let mut spinner3 = Spinner::new_with_stream(
        spinners::Dots,
        "正在更新...",
        Color::Yellow,
        Streams::Stderr,
    );
    if let Ok(true) = updater(&root).await {
        spinner3.stop_and_persist("✔️", "更新成功");
    } else {
        spinner3.stop_and_persist("❌", "更新失败");
        start_maa(&root);
        wait_enter_and_exit();
    }
    let mut spinner4 = Spinner::new_with_stream(
        spinners::Dots,
        "正在解压...",
        Color::Yellow,
        Streams::Stderr,
    );
    // if let Ok(true) = extract_update_resource(&get_current_dir()).await {
    //     spinner4.stop_and_persist("✔️", "解压成功");
    // } else {
    //     spinner4.stop_and_persist("❌", "解压失败");
    //     wait_enter_and_exit();
    // }

    match extract_update_resource(&root) {
        Ok(true) => {
            spinner4.stop_and_persist("✔️", "解压成功");
        }
        Err(e) => {
            spinner4.stop_and_persist("❌", "解压失败");
            println!("错误信息：{}", e);
            start_maa(&root);
            wait_enter_and_exit();
        }
        _ => {}
    }

    let mut spinner5 = Spinner::new_with_stream(
        spinners::Dots,
        "正在更新上次更新时间...",
        Color::Yellow,
        Streams::Stderr,
    );
    let now = Local::now().timestamp();
    let mut file =
        File::create(root.join("last_update_time.txt")).expect("无法创建上次更新时间文件");
    file.write_all(now.to_string().as_bytes())
        .expect("无法写入上次更新时间文件");
    spinner5.stop_and_persist("✔️", "更新上次更新时间成功");
    // 删除更新包文件
    fs::remove_file(root.join("updateResource.zip")).expect("无法删除更新包文件");
    // 启动MAA
    start_maa(&root);

    // println!("Hello, world!");
}
