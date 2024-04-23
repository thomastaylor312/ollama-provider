#![allow(clippy::missing_safety_doc)]
wit_bindgen::generate!();

use std::io::{Read, Write};

mod helpers;

use helpers::*;

use exports::wasi::http::incoming_handler::Guest;
use thomastaylor312::ollama::generate::{generate, Request};
use wasi::http::types::*;

struct HttpServer;

impl Guest for HttpServer {
    fn handle(request: IncomingRequest, response_out: ResponseOutparam) {
        let incoming_req_body = request
            .consume()
            .expect("failed to consume incoming request body");
        let mut incoming_req_body_stream = incoming_req_body
            .stream()
            .expect("failed to get incoming request body stream");
        let mut reader = InputStreamReader::from(&mut incoming_req_body_stream);
        let prompt = {
            let mut buf = String::new();
            reader.read_to_string(&mut buf).unwrap();
            drop(incoming_req_body_stream);
            buf
        };

        let resp = generate(&Request {
            prompt,
            images: None,
        })
        .expect("Unable to generate");
        let response = OutgoingResponse::new(Fields::new());
        response.set_status_code(200).unwrap();
        let response_body = response.body().unwrap();

        let mut out = response_body.write().unwrap();
        let mut writer = OutputStreamWriter::from(&mut out);
        writer.write_all(resp.response.as_bytes()).unwrap();
        drop(out);
        OutgoingBody::finish(response_body, None).expect("failed to finish response body");
        ResponseOutparam::set(response_out, Ok(response));
    }
}

export!(HttpServer);
