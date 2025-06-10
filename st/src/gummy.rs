use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use log::debug;
use std::result::Result::Ok;
use std::vec;
use tokio_tungstenite::{WebSocketStream, connect_async_tls_with_config};
use tungstenite::Message;
use tungstenite::client::IntoClientRequest;

mod request {
    use chrono::format;
    use dasp::sample;
    use serde::Deserialize;
    use serde::Serialize;

    #[derive(Serialize, Deserialize)]
    pub struct Header {
        task_id: String,
        action: String,
        streaming: String,
    }

    #[derive(Serialize, Deserialize)]
    pub struct Parameters {
        sample_rate: u32,
        format: String,
        source_language: Option<String>,
        transcription_enabled: bool,
        translation_enabled: bool,
        translation_target_languages: Vec<String>,
    }

    #[derive(Serialize, Deserialize)]
    pub struct Input {}

    #[derive(Serialize, Deserialize)]
    pub struct Payload {
        model: Option<String>,
        parameters: Option<Parameters>,
        input: Input,
        task: Option<String>,
        task_group: Option<String>,
        function: Option<String>,
    }

    #[derive(Serialize, Deserialize)]
    pub struct StartMessage {
        header: Header,
        payload: Payload,
    }

    impl StartMessage {
        pub fn new(
            format: Option<&str>,
            sample_rate: Option<u32>,
            source_language: Option<&str>,
            target_language: Option<&str>,
        ) -> Self {
            let task_id = uuid::Uuid::new_v4().to_string();
            let format = format.map(|s| s.to_string()).unwrap_or("pcm".to_string());
            let sample_rate = sample_rate.unwrap_or(48000);
            let source_language = source_language
                .map(|s| s.to_string())
                .unwrap_or("auto".to_string());
            let target_language = target_language
                .map(|s| s.to_string())
                .unwrap_or("zh".to_string());
            StartMessage {
                header: Header {
                    task_id: task_id.to_string(),
                    action: "run-task".to_string(),
                    streaming: "duplex".to_string(),
                },
                payload: Payload {
                    model: Some("gummy-realtime-v1".to_string()),
                    parameters: Some(Parameters {
                        sample_rate: sample_rate,
                        format: format,
                        source_language: Some(source_language),
                        transcription_enabled: true,
                        translation_enabled: true,
                        translation_target_languages: vec![target_language],
                    }),
                    input: Input {},
                    task: Some("asr".to_string()),
                    task_group: Some("audio".to_string()),
                    function: Some("recognition".to_string()),
                },
            }
        }

        pub fn id(&self) -> &str {
            &self.header.task_id
        }
    }

    #[derive(Serialize, Deserialize)]
    pub struct FinishMessage {
        header: Header,
        payload: Payload,
    }

    impl FinishMessage {
        pub fn new(task_id: &str) -> Self {
            FinishMessage {
                header: Header {
                    task_id: task_id.to_string(),
                    action: "finish-task".to_string(),
                    streaming: "duplex".to_string(),
                },
                payload: Payload {
                    model: None,
                    parameters: None,
                    input: Input {},
                    task: None,
                    task_group: None,
                    function: None,
                },
            }
        }
        pub fn id(&self) -> &str {
            &self.header.task_id
        }
    }
}

type WSWriter =
    SplitSink<WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, Message>;

type WSReader =
    SplitStream<WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>>;

pub struct Closed;

pub struct Connected {
    writer: WSWriter,
    reader: WSReader,
}

pub struct Sending {
    writer: WSWriter,
    reader: WSReader,
    task_id: String,
}

pub struct Received {
    writer: WSWriter,
    reader: WSReader,
    task_id: String,
    result: Vec<String>,
}

pub struct Gummy<State = Closed> {
    api_key: String,
    state: State,
}

impl Gummy {
    pub fn new(api_key: &str) -> Self {
        Gummy {
            api_key: api_key.to_string(),
            state: Closed,
        }
    }
    pub fn close(self) -> Gummy<Closed> {
        Gummy {
            api_key: self.api_key,
            state: Closed,
        }
    }
}

impl Gummy<Closed> {
    pub async fn connect(self, url: Option<&str>) -> Result<Gummy<Connected>, anyhow::Error> {
        let url = url.unwrap_or("wss://dashscope.aliyuncs.com/api-ws/v1/inference");
        let mut request = url.into_client_request()?;
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
        let state = Connected { writer, reader };
        Ok(Gummy {
            api_key: self.api_key,
            state,
        })
    }
}

impl Gummy<Connected> {
    pub async fn start(
        mut self,
        format: Option<&str>,
        sample_rate: Option<u32>,
        source_language: Option<&str>,
        target_language: Option<&str>,
    ) -> Result<Gummy<Sending>, anyhow::Error> {
        let start_message =
            request::StartMessage::new(format, sample_rate, source_language, target_language);
        self.state
            .writer
            .send(Message::Text(
                serde_json::to_string(&start_message).unwrap().into(),
            ))
            .await?;
        while let Some(message) = self.state.reader.next().await {
            match message {
                Ok(Message::Text(text)) => {
                    debug!(
                        "[{}] Received message: {}",
                        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                        text
                    );
                    let response: serde_json::Value = serde_json::from_str(&text)?;
                    let event = response["header"]["event"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("Invalid response format"))?;
                    let task_id_response = response["header"]["task_id"]
                        .as_str()
                        .expect("Missing task_id in response")
                        .to_string();

                    if event == "task-started" && task_id_response == start_message.id() {
                        debug!("Task started with ID: {}", start_message.id());
                        break;
                    }
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Error receiving message: {}", e));
                }
                _ => {
                    debug!("Received non-text message, ignoring.");
                }
            }
        }
        let state = Sending {
            writer: self.state.writer,
            reader: self.state.reader,
            task_id: start_message.id().to_string(),
        };
        Ok(Gummy {
            api_key: self.api_key,
            state,
        })
    }
}

impl Gummy<Sending> {
    pub async fn send(&mut self, data: &[u8]) -> Result<(), anyhow::Error> {
        self.state
            .writer
            .send(Message::Binary(data.to_vec().into()))
            .await?;
        Ok(())
    }

    pub async fn finish(mut self) -> Result<Gummy<Received>, anyhow::Error> {
        let message = request::FinishMessage::new(&self.state.task_id);
        self.state
            .writer
            .send(Message::Text(
                serde_json::to_string(&message).unwrap().into(),
            ))
            .await?;
        let mut result = vec![];
        while let Some(message) = self.state.reader.next().await {
            match message {
                Ok(Message::Text(text)) => {
                    debug!(
                        "[{}] Received message: {}",
                        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                        text
                    );
                    let response: serde_json::Value = serde_json::from_str(&text)?;
                    let event = response["header"]["event"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("Invalid response format"))?;
                    let task_id = response["header"]["task_id"]
                        .as_str()
                        .expect("Missing task_id in response")
                        .to_string();

                    if event == "result-generated" && task_id == self.state.task_id {
                        let sentence_end =
                            response["payload"]["output"]["transcription"]["sentence_end"]
                                .as_bool()
                                .expect("Missing sentence_end in response");
                        let text = response["payload"]["output"]["transcription"]["text"]
                            .as_str()
                            .expect("Missing text in response")
                            .to_string();
                        debug!("Received translation: {}", text);
                        result.push(text);
                        if sentence_end {
                            debug!("Sentence ended, stopping.");
                        }
                    }
                    if event == "task-finished" && task_id == self.state.task_id {
                        debug!("Task finished with ID: {}", task_id);
                        break;
                    }
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Error receiving message: {}", e));
                }
                _ => {
                    debug!("Received non-text message, ignoring.");
                }
            }
        }
        let state = Received {
            task_id: self.state.task_id,
            result: result, // Placeholder for result
            writer: self.state.writer,
            reader: self.state.reader,
        };

        Ok(Gummy {
            api_key: self.api_key,
            state,
        })
    }
}

impl Gummy<Received> {
    pub async fn start(
        mut self,
        format: Option<&str>,
        sample_rate: Option<u32>,
        source_language: Option<&str>,
        target_language: Option<&str>,
    ) -> Result<Gummy<Sending>, anyhow::Error> {
        let message =
            request::StartMessage::new(format, sample_rate, source_language, target_language);
        self.state
            .writer
            .send(Message::Text(
                serde_json::to_string(&message).unwrap().into(),
            ))
            .await?;
        let state = Sending {
            writer: self.state.writer,
            reader: self.state.reader,
            task_id: self.state.task_id.clone(),
        };
        Ok(Gummy {
            api_key: self.api_key,
            state,
        })
    }

    pub fn get_result(&self) -> Vec<String> {
        self.state.result.clone()
    }
}
