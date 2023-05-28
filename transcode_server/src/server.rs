/*
 * server.rs
 *
 * This file contains code for transcoding a video using ffmpeg.
 * Upload a video in h264 format and it will be encrypted and transcoded to 2 av1 files,
 * one in 2160p format and another in 1080p.
 * This is then uploaded to decentralised SIA Storage via S5.
 *
 * Author: Jules Lai
 * Date: 28 May 2023
 */

mod s5;
use s5::{download_file, upload_video};

mod encrypt_file;
use encrypt_file::encrypt_file_xchacha20;

use tonic::{transport::Server, Code, Request, Response, Status};

use async_trait::async_trait;
use once_cell::sync::Lazy;
use s5::hash_blake3_file;
use sanitize_filename::sanitize;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use transcode::{
    transcode_service_server::{TranscodeService, TranscodeServiceServer},
    GetCidRequest, GetCidResponse, TranscodeRequest, TranscodeResponse,
};
mod encrypted_cid;
use base64::{engine::general_purpose, Engine as _};
use encrypted_cid::create_encrypted_cid;

use std::path::Path;

use dotenv::dotenv;

static VIDEO_CID: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::from("")));
static VIDEO_CID1: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::from("")));
static VIDEO_CID2: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::from("")));
static PATH_TO_FILE: &str = "path/to/file/";

// The transcoding task receiver, which receives transcoding tasks from the gRPC server
async fn transcode_task_receiver(receiver: Arc<Mutex<mpsc::Receiver<(String, bool)>>>) {
    while let Some((file_path, is_gpu)) = receiver.lock().await.recv().await {
        println!("Transcoding video: {}", &file_path);
        if let Err(e) = transcode_video(&file_path, is_gpu).await {
            eprintln!("Failed to transcode {}: {}", &file_path, e);
        }
    }
}

fn bytes_to_base64url(bytes: &[u8]) -> String {
    let engine = general_purpose::STANDARD_NO_PAD;

    let mut base64_string = engine.encode(bytes);

    // Replace standard base64 characters with URL-safe ones
    base64_string = base64_string.replace("+", "-").replace("/", "_");

    base64_string
}

pub fn hash_bytes_to_cid(hash: Vec<u8>, file_size: u64) -> Vec<u8> {
    // Decode the base64url hash back to bytes
    // Prepend the byte 0x26 before the full hash
    let mut bytes = hash.to_vec();
    bytes.insert(0, 0x1f);
    bytes.insert(0, 0x26);

    // Append the size of the file, little-endian encoded
    let le_file_size = &file_size.to_le_bytes();
    let mut trimmed = le_file_size.as_slice();

    // Remove the trailing zeros
    while let Some(0) = trimmed.last() {
        trimmed = &trimmed[..trimmed.len() - 1];
    }

    bytes.extend(trimmed);

    bytes
}

// Transcodes a video file to 2160p and 1080p h264 av1 formats using ffmpeg
async fn transcode_video(url: &str, is_gpu: bool) -> Result<Response<TranscodeResponse>, Status> {
    println!("Downloading video from: {}", url);

    let mut video_cid = VIDEO_CID.lock().await;
    *video_cid = url.to_string();

    let file_name = sanitize(url);
    let file_path = String::from(PATH_TO_FILE.to_owned() + &file_name);

    match download_file(url, file_path.as_str()) {
        Ok(()) => println!("File downloaded successfully"),
        Err(e) => eprintln!("Error downloading file: {}", e),
    }

    println!("Transcoding video: {}", &file_path);
    println!("is_gpu = {}", &is_gpu);

    let mut encryption_key1: Vec<u8> = Vec::new();
    let mut encryption_key2: Vec<u8> = Vec::new();

    if is_gpu {
        println!("GPU transcoding");
        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-i",
            file_path.as_str(),
            "-c:v",
            "av1_nvenc",
            "-b:v",
            "15M",
            "-c:a",
            "libopus",
            "-b:a",
            "192k",
            "-ac",
            "2",
            "-vf",
            "scale=3840:2160",
            "-y",
            format!("./temp/to/transcode/{}_2160p_ue.mp4", &file_name).as_str(),
        ]);
        let output = cmd.output().expect("Failed to execute command");
        println!("{:?}", output);

        match encrypt_file_xchacha20(
            format!("./temp/to/transcode/{}_2160p_ue.mp4", file_name),
            format!("./temp/to/transcode/{}_2160p.mp4", file_name),
            0,
        ) {
            Ok(bytes) => {
                // Encryption succeeded, and `bytes` contains the encrypted data
                // Add your success handling code here
                encryption_key1 = bytes;
                println!("Encryption succeeded");
            }
            Err(error) => {
                // Encryption failed
                // Handle the error here
                eprintln!("Encryption error: {:?}", error);
                // Optionally, you can return an error or perform error-specific handling
            }
        }

        println!("Before2: let cmd = format!(");

        let mut cmd2 = Command::new("ffmpeg");
        cmd2.args([
            "-i",
            file_path.as_str(),
            "-c:v",
            "av1_nvenc",
            "-b:v",
            "5M",
            "-c:a",
            "libopus",
            "-b:a",
            "96k",
            "-ac",
            "2",
            "-vf",
            "scale=1920:1080",
            "-y",
            format!("./temp/to/transcode/{}_1080p_ue.mp4", &file_name).as_str(),
        ]);

        let output2 = cmd2.output().expect("Failed to execute command");
        println!("{:?}", output2);

        match encrypt_file_xchacha20(
            format!("./temp/to/transcode/{}_1080p_ue.mp4", file_name),
            format!("./temp/to/transcode/{}_1080p.mp4", file_name),
            0,
        ) {
            Ok(bytes) => {
                // Encryption succeeded, and `bytes` contains the encrypted data
                // Add your success handling code here
                encryption_key2 = bytes;
                println!("Encryption succeeded");
            }
            Err(error) => {
                // Encryption failed
                // Handle the error here
                eprintln!("Encryption error: {:?}", error);
                // Optionally, you can return an error or perform error-specific handling
            }
        }
    } else {
        println!("CPU transcoding");

        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-i",
            file_path.as_str(),
            "-c:v",
            "libaom-av1", // use libaom-av1 encoder for AV1
            "-cpu-used",
            "4", // set encoding speed to 4 (range 0-8, lower is slower)
            "-b:v",
            "0", // use constant quality mode
            "-crf",
            "30", // set quality level to 30 (range 0-63, lower is better)
            "-c:a",
            "libopus", // use libopus encoder for audio
            "-b:a",
            "128k",
            "-ac",
            "2",
            "-s",
            "hd1080",
            "-y",
            format!("./temp/to/transcode/{}_2160p.mp4", &file_name).as_str(), // change output extension to .av1
        ]);
        let output = cmd.output().expect("Failed to execute command");
        println!("{:?}", output);

        println!("Before2: let cmd = format!(");

        let mut cmd2 = Command::new("ffmpeg");
        cmd2.args([
            "-i",
            file_path.as_str(),
            "-c:v",
            "libaom-av1", // use libaom-av1 encoder for AV1
            "-cpu-used",
            "4", // set encoding speed to 4 (range 0-8, lower is slower)
            "-b:v",
            "0", // use constant quality mode
            "-crf",
            "30", // set quality level to 30 (range 0-63, lower is better)
            "-c:a",
            "libopus", // use libopus encoder for audio
            "-b:a",
            "128k",
            "-ac",
            "2",
            "-s",
            "hd720",
            "-y",
            format!("./temp/to/transcode/{}_1080p.mp4", &file_name).as_str(), // change output extension to .av1
        ]);

        let output2 = cmd2.output().expect("Failed to execute command");
        println!("{:?}", output2);
    }

    let file_path = format!("./temp/to/transcode/{}_2160p_ue.mp4", file_name);
    let file_path_encrypted = format!("./temp/to/transcode/{}_2160p.mp4", file_name);

    let hash_result = hash_blake3_file(file_path.clone());
    let hash_result_encrypted = hash_blake3_file(file_path_encrypted);

    let cid_type_encrypted: u8 = 0xae; // replace with your actual cid type encrypted
    let encryption_algorithm: u8 = 0xa6; // replace with your actual encryption algorithm
    let chunk_size_as_power_of_2: u8 = 18; // replace with your actual chunk size as power of 2
    let padding: u32 = 0; // replace with your actual padding

    // Upload the transcoded videos to storage
    let mut response: TranscodeResponse;
    match upload_video(format!("./temp/to/transcode/{}_2160p.mp4", file_name).as_str()) {
        Ok(cid) => {
            println!(
                "******************************************2160p cid: {:?}",
                &cid
            );

            let mut hash = Vec::new();
            match hash_result {
                Ok(hash1) => {
                    hash = hash1.as_bytes().to_vec();
                    // Now you can use bytes as needed.
                }
                Err(err) => {
                    eprintln!("Error computing blake3 hash: {}", err);

                    return Err(Status::new(
                        Code::Internal,
                        format!("Error computing blake3 hash: {}", err),
                    ));
                }
            }

            let mut hash_encrypted = Vec::new();
            match hash_result_encrypted {
                Ok(hash1) => {
                    hash_encrypted = hash1.as_bytes().to_vec();
                    // Now you can use bytes as needed.
                }
                Err(err) => {
                    eprintln!("Error computing blake3 hash: {}", err);

                    return Err(Status::new(
                        Code::Internal,
                        format!("Error computing blake3 hash: {}", err),
                    ));
                }
            }

            let mut encrypted_blob_hash = vec![0x1f];
            encrypted_blob_hash.extend(hash_encrypted);

            let cloned_hash = encrypted_blob_hash.clone();

            let file_path_path = Path::new(&file_path);
            let metadata = std::fs::metadata(file_path_path).expect("Failed to read metadata");
            let file_size = metadata.len();

            let cid_ue = hash_bytes_to_cid(hash, file_size);

            println!("encryption_key1: {:?}", encryption_key1);
            println!("cid: {:?}", cid);
            println!("cid_ue: {:?}", cid_ue);
            let encrypted_cid_bytes = create_encrypted_cid(
                cid_type_encrypted,
                encryption_algorithm,
                chunk_size_as_power_of_2,
                encrypted_blob_hash,
                encryption_key1,
                padding,
                cid_ue,
            );

            let encrypted_cid = format!("u{}", bytes_to_base64url(&encrypted_cid_bytes));

            // Now you have your encrypted_blob_hash and encrypted_cid
            println!("Encrypted Blob Hash: {:02x?}", cloned_hash);
            println!("Encrypted CID: {:?}", encrypted_cid);
            let mut video_cid1 = VIDEO_CID1.lock().await;
            *video_cid1 = encrypted_cid;

            println!("Transcoding task finished");
        }
        Err(e) => {
            println!("!!!!!!!!!!!!!!!!!!!!!2160p no cid");
            println!("Error: {}", e); // This line is added to print out the error message

            return Err(Status::new(
                Code::Internal,
                format!("Transcoding task failed with error {}", e),
            ));
        }
    };

    match upload_video(format!("./temp/to/transcode/{}_1080p.mp4", file_name).as_str()) {
        Ok(cid) => {
            response = TranscodeResponse {
                status_code: 200,
                message: "Transcoding task finished".to_string(),
            };

            // Instantiate an original CID and a Multihash
            let file_path = format!("./temp/to/transcode/{}_1080p_ue.mp4", file_name);
            let file_path_encrypted = format!("./temp/to/transcode/{}_1080p.mp4", file_name);

            let hash_result = hash_blake3_file(file_path.clone());
            let hash_result_encrypted = hash_blake3_file(file_path_encrypted);

            let mut hash = Vec::new();
            match hash_result {
                Ok(hash1) => {
                    hash = hash1.as_bytes().to_vec();
                }
                Err(err) => {
                    eprintln!("Error computing blake3 hash: {}", err);

                    response = TranscodeResponse {
                        status_code: 500,
                        message: format!("Error computing blake3 hash: {}", err),
                    };
                }
            }

            let mut hash_encrypted = Vec::new();
            match hash_result_encrypted {
                Ok(hash1) => {
                    hash_encrypted = hash1.as_bytes().to_vec();
                }
                Err(err) => {
                    eprintln!("Error computing blake3 hash: {}", err);

                    response = TranscodeResponse {
                        status_code: 500,
                        message: format!("Error computing blake3 hash: {}", err),
                    };
                }
            }

            let mut encrypted_blob_hash = vec![0x1f];
            encrypted_blob_hash.extend(hash_encrypted);

            let cloned_hash = encrypted_blob_hash.clone();

            let file_path_path_ue = Path::new(&file_path);
            let metadata = std::fs::metadata(file_path_path_ue).expect("Failed to read metadata");
            let file_size = metadata.len();

            let cid_ue = hash_bytes_to_cid(hash, file_size);

            println!("encryption_key2: {:?}", encryption_key2);
            println!("cid: {:?}", cid);
            println!("cid_ue: {:?}", cid_ue);
            let encrypted_cid_bytes = create_encrypted_cid(
                cid_type_encrypted,
                encryption_algorithm,
                chunk_size_as_power_of_2,
                encrypted_blob_hash,
                encryption_key2,
                padding,
                cid_ue,
            );

            let encrypted_cid = format!("u{}", bytes_to_base64url(&encrypted_cid_bytes));

            // Now you have your encrypted_blob_hash and encrypted_cid
            println!("Encrypted Blob Hash: {:02x?}", cloned_hash);
            println!("Encrypted CID: {:?}", encrypted_cid);

            let mut video_cid2 = VIDEO_CID2.lock().await;
            println!("after video_cid2");
            *video_cid2 = encrypted_cid;
            println!("after *video_cid2 = cid");
        }
        Err(e) => {
            println!("!!!!!!!!!!!!!!!!!!!!!1080p no cid");
            println!("Error: {}", e); // This line is added to print out the error message

            return Err(Status::new(
                Code::Internal,
                format!("Transcoding task failed with error {}", e),
            ));
        }
    };

    Ok(Response::new(response))
}

// The gRPC service implementation
#[derive(Debug, Clone)]
struct TranscodeServiceHandler {
    transcode_task_sender: Option<Arc<Mutex<mpsc::Sender<(String, bool)>>>>,
}

#[async_trait]
impl TranscodeService for TranscodeServiceHandler {
    async fn transcode(
        &self,
        request: Request<TranscodeRequest>,
    ) -> Result<Response<TranscodeResponse>, Status> {
        let url = request.get_ref().url.to_string();
        println!("Received URL: {}", url);

        let is_gpu = request.get_ref().is_gpu;
        println!("Received is_gpu: {}", is_gpu);

        println!(
            "transcode_task_sender is None: {}",
            self.transcode_task_sender.is_none()
        );
        // Send the transcoding task to the transcoding task receiver
        if let Some(ref sender) = self.transcode_task_sender {
            let sender = sender.lock().await.clone();
            if let Err(e) = sender.send((url, is_gpu)).await {
                return Err(Status::internal(format!(
                    "Failed to send transcoding task: {}",
                    e
                )));
            }
        }

        let response = TranscodeResponse {
            status_code: 200,
            message: "Transcoding task queued".to_string(),
        };

        Ok(Response::new(response))
    }

    async fn get_cid(
        &self,
        request: Request<GetCidRequest>,
    ) -> Result<Response<GetCidResponse>, Status> {
        let resolution = request.get_ref().resolution.as_str();

        // Assuming `resolution` is already defined and contains the resolution value
        let cid_option = match resolution {
            "2160p" => Some(VIDEO_CID1.lock().await.to_string()),
            "1080p" => Some(VIDEO_CID2.lock().await.to_string()),
            _ => None,
        };

        let cid_option_clone = cid_option.clone();
        let cid = cid_option_clone.unwrap_or_default();

        let response = GetCidResponse {
            status_code: if cid_option.is_some() { 200 } else { 404 },
            cid,
        };
        println!(
            "get_cid Response: {}, {}",
            response.status_code, response.cid
        );

        Ok(Response::new(response))
    }
}

impl Drop for TranscodeServiceHandler {
    fn drop(&mut self) {
        self.transcode_task_sender = None;
    }
}

pub mod transcode {
    tonic::include_proto!("transcode");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    // Create a channel for transcoding tasks
    let (task_sender, task_receiver) = mpsc::channel::<(String, bool)>(100);
    let task_receiver = Arc::new(Mutex::new(task_receiver));

    // Start the transcoding task receiver
    let receiver_clone = Arc::clone(&task_receiver);
    tokio::spawn(transcode_task_receiver(receiver_clone));

    // Create a gRPC server
    let addr = "0.0.0.0:50051".parse()?;

    // Wrap task_sender in an Arc<Mutex<>> before passing it to TranscodeServiceHandler
    let task_sender = Arc::new(Mutex::new(task_sender));

    let transcode_service_handler = TranscodeServiceHandler {
        transcode_task_sender: Some(task_sender),
    };
    let transcode_service_server = TranscodeServiceServer::new(transcode_service_handler);
    Server::builder()
        .add_service(transcode_service_server)
        .serve(addr)
        .await?;

    Ok(())
}
