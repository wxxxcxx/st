use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::result::Result::Ok;
use tokio_tungstenite::{WebSocketStream, connect_async_tls_with_config};
use tungstenite::Message;
use tungstenite::client::IntoClientRequest;

#[derive(Serialize, Deserialize)]
pub struct RequestHeader {
    task_id: String,
    action: String,
    streaming: String,
}

#[derive(Serialize, Deserialize)]
pub struct RequestParameters {
    sample_rate: u32,
    format: String,
    source_language: Option<String>,
    transcription_enabled: bool,
    translation_enabled: bool,
    translation_target_languages: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct RequestInput {}

#[derive(Serialize, Deserialize)]
pub struct RequestPayload {
    model: Option<String>,
    parameters: Option<RequestParameters>,
    input: RequestInput,
    task: Option<String>,
    task_group: Option<String>,
    function: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct RequestMessage {
    header: RequestHeader,
    payload: RequestPayload,
}

impl RequestMessage {
    pub fn new(event: &str) -> Self {
        match event {
            "run-task" => RequestMessage {
                header: RequestHeader {
                    task_id: uuid::Uuid::new_v4().to_string(),
                    action: "run-task".to_string(),
                    streaming: "duplex".to_string(),
                },
                payload: RequestPayload {
                    model: Some("gummy-realtime-v1".to_string()),
                    parameters: Some(RequestParameters {
                        sample_rate: 16000,
                        format: "pcm".to_string(),
                        source_language: None,
                        transcription_enabled: true,
                        translation_enabled: true,
                        translation_target_languages: vec!["en".to_string()],
                    }),
                    input: RequestInput {},
                    task: Some("asr".to_string()),
                    task_group: Some("audio".to_string()),
                    function: Some("recognition".to_string()),
                },
            },
            _ => panic!("Unsupported event type: {}", event),
        }
    }
}

type WSWriter =
    SplitSink<WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, Message>;

type WSReader =
    SplitStream<WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>>;

pub struct Initial {
    api_key: String,
}

pub struct Ready {
    writer: WSWriter,
    reader: WSReader,
}

pub struct Processing {
    writer: WSWriter,
    reader: WSReader,
    task_id: String,
}

pub enum Gummy {
    Ready(Ready),
    Processing(Processing),
    Closed,
    Error(String),
}

impl Gummy {
    pub async fn connect(api_key: String) -> Result<Gummy, anyhow::Error> {
        let mut request =
            "wss://dashscope.aliyuncs.com/api-ws/v1/inference".into_client_request()?;
        request
            .headers_mut()
            .insert("Authorization", format!("Bearer {}", api_key).parse()?);
        request.headers_mut().insert("user-agent", "app".parse()?);
        request
            .headers_mut()
            .insert("X-DashScope-WorkSpace", "llm-hxfupix3oo63uw6d".parse()?);
        request
            .headers_mut()
            .insert("X-DashScope-DataInspection", "enable".parse()?);
        let (stream, _) = connect_async_tls_with_config(request, None, false, None).await?;
        let (writer, reader) = stream.split();
        Ok(Gummy::Ready(Ready { writer, reader }))
    }
    pub async fn start(self, data: &[u8]) -> Result<Self, anyhow::Error> {
        match self {
            Gummy::Ready(mut ready) => loop {
                let message = RequestMessage::new("run-task");
                ready
                    .writer
                    .send(Message::Text(
                        serde_json::to_string(&message).unwrap().into(),
                    ))
                    .await?;

                let message = ready.reader.next().await.unwrap()?;
                if let Message::Text(text) = message {
                    println!(
                        "[{}] Received initial message: {}",
                        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                        text
                    );
                    let response: serde_json::Value = serde_json::from_str(&text)?;
                    let event = response["header"]["event"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("Invalid response format"))?;
                    if event == "task-started" {
                        println!("Task started successfully.");
                        return Ok(Gummy::Processing(Processing {
                            writer: ready.writer,
                            reader: ready.reader,
                            task_id: response["header"]["task_id"]
                                .as_str()
                                .unwrap_or_default()
                                .to_string(),
                        }));
                    } else {
                        return Ok(Gummy::Error(format!("Unexpected event: {}", event)));
                    }
                } else {
                    panic!("Unexpected message type: {:?}", message);
                }
            },
            Gummy::Processing(mut processing) => {
                println!(
                    "[{}] Sending initial data...",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
                );
                processing
                    .writer
                    .send(Message::Binary(data.to_vec().into()))
                    .await?;
                processing
                    .writer
                    .send(Message::Text(
                        serde_json::to_string(&RequestMessage::new("finish-task"))
                            .unwrap()
                            .into(),
                    ))
                    .await?;
                Ok(Gummy::Processing(processing))
            }
            _ => Err(anyhow::anyhow!("Gummy is not in a ready state.")),
        }
    }

    pub async fn bind(
        self,
        mut rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
    ) -> Result<Self, anyhow::Error> {
        match self {
            Gummy::Processing(mut processing) => {
                loop {
                    tokio::select! {
                       send_data = rx.recv() => {
                            println!(
                                "[{}] Sending data...",
                                chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
                            );
                            if let Some(data) = send_data {
                                if let Err(e) = processing.writer.send(Message::Binary(data.into())).await {
                                    eprintln!("Error sending data: {}", e);
                                }
                            } else {
                                println!("No more data to send, exiting.");
                                break;
                            }
                        },
                        message = processing.reader.next() => {
                            println!(
                                "[{}] Receiving message...",
                                chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
                            );
                            match message {
                                Some(Result::Ok(Message::Text(text))) => {
                                    println!(
                                        "[{}] Received message: {}",
                                        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                                        text
                                    );
                                    let data: Vec<i32> = serde_json::from_str(&text)?;
                                    // Process the received data as needed
                                    println!("Received data: {:?}", data);
                                }
                                _ => {
                                    eprintln!("Received unexpected message type or error.");
                                    continue;
                                }
                            }
                        },
                    }
                }
                Ok(Gummy::Closed)
            }
            _ => Err(anyhow::anyhow!("Gummy is not in a ready state.")),
        }
    }
}
