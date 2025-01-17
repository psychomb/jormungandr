use hex;
use jcli_app::utils::{open_api_verifier, DebugFlag, OpenApiVerifier};
use reqwest::{self, Client, RequestBuilder, Response};
use serde::Serialize;
use std::{fmt, io::Write};

pub const DESERIALIZATION_ERROR_MSG: &'static str = "node returned malformed data";

pub struct RestApiSender<'a> {
    builder: RequestBuilder,
    request_body_debug: Option<String>,
    debug_flag: &'a DebugFlag,
}

pub struct RestApiResponse {
    body: RestApiResponseBody,
    response: Response,
}

pub enum RestApiResponseBody {
    Text(String),
    Binary(Vec<u8>),
}

custom_error! { pub Error
    RequestFailed { source: reqwest::Error } = @{ reqwest_error_msg(source) },
    VerificationFailed { source: open_api_verifier::Error } = "request didn't pass verification",
}

fn reqwest_error_msg(err: &reqwest::Error) -> &'static str {
    if err.is_timeout() {
        "connection with node timed out"
    } else if err.is_http() {
        "could not connect with node"
    } else if err.is_serialization() {
        DESERIALIZATION_ERROR_MSG
    } else if err.is_redirect() {
        "redirecting error while connecting with node"
    } else if err.is_client_error() {
        "node rejected request because of invalid parameters"
    } else if err.is_server_error() {
        "node internal error"
    } else {
        "communication with node failed in unexpected way"
    }
}

impl<'a> RestApiSender<'a> {
    pub fn new(builder: RequestBuilder, debug_flag: &'a DebugFlag) -> Self {
        Self {
            builder,
            request_body_debug: None,
            debug_flag,
        }
    }

    pub fn with_binary_body(mut self, body: Vec<u8>) -> Self {
        if self.debug_flag.debug_writer().is_some() {
            self.request_body_debug = Some(hex::encode(&body));
        }
        self.builder = self
            .builder
            .header(
                reqwest::header::CONTENT_TYPE,
                mime::APPLICATION_OCTET_STREAM.as_ref(),
            )
            .body(body);
        self
    }

    pub fn with_json_body(mut self, body: &impl Serialize) -> Result<Self, serde_json::Error> {
        let json = serde_json::to_string(body)?;
        if self.debug_flag.debug_writer().is_some() {
            self.request_body_debug = Some(json.clone());
        }
        self.builder = self
            .builder
            .header(
                reqwest::header::CONTENT_TYPE,
                mime::APPLICATION_JSON.as_ref(),
            )
            .body(json.into_bytes());
        Ok(self)
    }

    pub fn send(self) -> Result<RestApiResponse, Error> {
        let request = self.builder.build()?;
        if let Some(mut writer) = self.debug_flag.debug_writer() {
            writeln!(writer, "{:#?}", request).unwrap();
            if let Some(body) = self.request_body_debug {
                writeln!(writer, "Request body:\n{}", body).unwrap();
            }
        }
        OpenApiVerifier::load_from_env()?.verify_request(&request)?;
        let response_raw = Client::new().execute(request)?;
        let response = RestApiResponse::new(response_raw)?;
        if let Some(mut writer) = self.debug_flag.debug_writer() {
            writeln!(writer, "{:#?}", response.response()).unwrap();
            if !response.body().is_empty() {
                writeln!(writer, "Response body:\n{}", response.body()).unwrap();
            }
        }
        Ok(response)
    }
}

impl RestApiResponse {
    pub fn new(mut response: Response) -> Result<Self, Error> {
        Ok(RestApiResponse {
            body: RestApiResponseBody::new(&mut response)?,
            response,
        })
    }

    pub fn response(&self) -> &Response {
        &self.response
    }

    pub fn ok_response(&self) -> Result<&Response, Error> {
        Ok(self.response().error_for_status_ref()?)
    }

    pub fn body(&self) -> &RestApiResponseBody {
        &self.body
    }
}

impl RestApiResponseBody {
    fn new(response: &mut Response) -> Result<Self, Error> {
        match is_body_binary(response) {
            true => {
                let mut data = Vec::with_capacity(response.content_length().unwrap_or(0) as usize);
                response.copy_to(&mut data)?;
                Ok(RestApiResponseBody::Binary(data))
            }
            false => {
                let data = response.text()?;
                Ok(RestApiResponseBody::Text(data))
            }
        }
    }

    pub fn text<'a>(&'a self) -> impl AsRef<str> + 'a {
        match self {
            RestApiResponseBody::Text(text) => text.into(),
            RestApiResponseBody::Binary(binary) => String::from_utf8_lossy(binary),
        }
    }

    pub fn binary(&self) -> &[u8] {
        match self {
            RestApiResponseBody::Text(text) => text.as_bytes(),
            RestApiResponseBody::Binary(binary) => binary,
        }
    }

    pub fn json<'a, T: serde::Deserialize<'a>>(&'a self) -> Result<T, serde_json::Error> {
        match self {
            RestApiResponseBody::Text(text) => serde_json::from_str(text),
            RestApiResponseBody::Binary(binary) => serde_json::from_slice(binary),
        }
    }

    pub fn json_value(&self) -> Result<serde_json::Value, serde_json::Error> {
        self.json()
    }

    pub fn is_empty(&self) -> bool {
        match self {
            RestApiResponseBody::Text(text) => text.is_empty(),
            RestApiResponseBody::Binary(binary) => binary.is_empty(),
        }
    }
}

impl fmt::Display for RestApiResponseBody {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            RestApiResponseBody::Text(text) => text.fmt(f),
            RestApiResponseBody::Binary(binary) => hex::encode(binary).fmt(f),
        }
    }
}

fn is_body_binary(response: &Response) -> bool {
    response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|header| header.to_str().ok())
        .and_then(|header_str| header_str.parse::<mime::Mime>().ok())
        == Some(mime::APPLICATION_OCTET_STREAM)
}
