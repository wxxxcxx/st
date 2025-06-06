use anyhow::Ok;
use async_trait::async_trait;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
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
    pub fn new(event: &str, task_id: &str) -> Self {
        match event {
            "run-task" => RequestMessage {
                header: RequestHeader {
                    task_id: task_id.to_string(),
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
            "finish-task" => RequestMessage {
                header: RequestHeader {
                    task_id: task_id.to_string(),
                    action: "finish-task".to_string(),
                    streaming: "duplex".to_string(),
                },
                payload: RequestPayload {
                    model: None,
                    parameters: None,
                    input: RequestInput {},
                    task: None,
                    task_group: None,
                    function: None,
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

pub struct Closed {
    api_key: String,
}

pub struct Connected {
    api_key: String,
    writer: WSWriter,
    reader: WSReader,
}

pub struct Transmitting {
    api_key: String,
    writer: WSWriter,
    reader: WSReader,
    task_id: String,
}

#[async_trait]
pub trait State {
    async fn connect(self: Box<Self>) -> Result<Box<dyn State>, anyhow::Error>;
    async fn start_task(self: Box<Self>) -> Result<Box<dyn State>, anyhow::Error>;
    async fn send_data(self: Box<Self>, data: &[u8]) -> Result<Box<dyn State>, anyhow::Error>;
    async fn finish_task(self: Box<Self>) -> Result<Box<dyn State>, anyhow::Error>;
    async fn close(self: Box<Self>) -> Result<Box<dyn State>, anyhow::Error>;
}

#[async_trait]
impl State for Closed {
    async fn connect(self: Box<Self>) -> Result<Box<dyn State>, anyhow::Error> {
        let mut request =
            "wss://dashscope.aliyuncs.com/api-ws/v1/inference".into_client_request()?;
        request
            .headers_mut()
            .insert("Authorization", format!("Bearer {}", self.api_key).parse()?);
        request.headers_mut().insert("user-agent", "app".parse()?);
        request
            .headers_mut()
            .insert("X-DashScope-WorkSpace", "llm-hxfupix3oo63uw6d".parse()?);
        request
            .headers_mut()
            .insert("X-DashScope-DataInspection", "enable".parse()?);
        let (stream, _) = connect_async_tls_with_config(request, None, false, None).await?;
        let (writer, reader) = stream.split();
        Ok(Box::new(Connected {
            api_key: self.api_key,
            writer,
            reader,
        }))
    }

    async fn start_task(self: Box<Self>) -> Result<Box<dyn State>, anyhow::Error> {
        todo!()
    }

    async fn send_data(self: Box<Self>, data: &[u8]) -> Result<Box<dyn State>, anyhow::Error> {
        todo!()
    }

    async fn finish_task(self: Box<Self>) -> Result<Box<dyn State>, anyhow::Error> {
        todo!()
    }

    async fn close(self: Box<Self>) -> Result<Box<dyn State>, anyhow::Error> {
        Ok(self)
    }
}

#[async_trait]
impl State for Connected {
    async fn connect(self: Box<Self>) -> Result<Box<dyn State>, anyhow::Error> {
        Err(anyhow::anyhow!("Already connected"))
    }

    async fn start_task(mut self: Box<Self>) -> Result<Box<dyn State>, anyhow::Error> {
        let task_id = uuid::Uuid::new_v4().to_string();
        let message = RequestMessage::new("run-task", &task_id);
        self.writer
            .send(Message::Text(
                serde_json::to_string(&message).unwrap().into(),
            ))
            .await?;
        Ok(Box::new(Transmitting {
            api_key: self.api_key,
            writer: self.writer,
            reader: self.reader,
            task_id,
        }))
    }

    async fn send_data(self: Box<Self>, data: &[u8]) -> Result<Box<dyn State>, anyhow::Error> {
        Err(anyhow::anyhow!("Cannot send data in connected state"))
    }

    async fn finish_task(self: Box<Self>) -> Result<Box<dyn State>, anyhow::Error> {
        Err(anyhow::anyhow!("Cannot finish task in connected state"))
    }

    async fn close(self: Box<Self>) -> Result<Box<dyn State>, anyhow::Error> {
        Ok(Box::new(Closed {
            api_key: self.api_key,
        }))
    }
}

#[async_trait]
impl State for Transmitting {
    async fn connect(self: Box<Self>) -> Result<Box<dyn State>, anyhow::Error> {
        Err(anyhow::anyhow!("Already in task"))
    }

    async fn start_task(self: Box<Self>) -> Result<Box<dyn State>, anyhow::Error> {
        Err(anyhow::anyhow!("Already in task"))
    }

    async fn send_data(mut self: Box<Self>, data: &[u8]) -> Result<Box<dyn State>, anyhow::Error> {
        self.writer
            .send(Message::Binary(data.to_vec().into()))
            .await?;
        Ok(self)
    }

    async fn finish_task(mut self: Box<Self>) -> Result<Box<dyn State>, anyhow::Error> {
        let message = RequestMessage::new("finish-task", &self.task_id);
        self.writer
            .send(Message::Text(
                serde_json::to_string(&message).unwrap().into(),
            ))
            .await?;
        Ok(Box::new(Connected {
            api_key: self.api_key,
            writer: self.writer,
            reader: self.reader,
        }))
    }

    async fn close(self: Box<Self>) -> Result<Box<dyn State>, anyhow::Error> {
        Ok(Box::new(Closed {
            api_key: self.api_key,
        }))
    }
}

pub enum Gummy {
    ReadyForTask(ReadyForTask),
    InTask(Transmitting),
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
        Ok(Gummy::ReadyForTask(ReadyForTask { writer, reader }))
    }
    pub async fn start_task(self) -> Result<Self, anyhow::Error> {
        match self {
            Gummy::ReadyForTask(mut ready) => {
                let task_id = uuid::Uuid::new_v4().to_string();
                let message = RequestMessage::new("run-task", &task_id);
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
                        return Ok(Gummy::InTask(Transmitting {
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
            }
            _ => Err(anyhow::anyhow!("Gummy is not in a ready state.")),
        }
    }

    pub async fn send_data(&mut self, data: &[u8]) -> Result<(), anyhow::Error> {
        match self {
            Gummy::InTask(in_task) => {
                println!(
                    "[{}] Sending data...",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
                );
                in_task
                    .writer
                    .send(Message::Binary(data.to_vec().into()))
                    .await?;
                Ok(())
            }
            _ => Err(anyhow::anyhow!("Gummy is not in task state.")),
        }
    }

    pub async fn finish_task(self) -> Result<Self, anyhow::Error> {
        match self {
            Gummy::InTask(mut in_task) => {
                let message = RequestMessage::new("finish_task", &in_task.task_id);
                println!(
                    "[{}] Finishing task...",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
                );
                in_task
                    .writer
                    .send(Message::Text(
                        serde_json::to_string(&message).unwrap().into(),
                    ))
                    .await?;
                Ok(Gummy::ReadyForTask(ReadyForTask {
                    writer: in_task.writer,
                    reader: in_task.reader,
                }))
            }
            _ => Err(anyhow::anyhow!("Gummy is not in a processing state.")),
        }
    }
}
