use std::io::{BufReader, Read};
use std::{io::{stdin, BufRead}, borrow::Cow};
use crossbeam_channel::bounded;
use flate2::bufread::{MultiGzDecoder};
use regex::bytes::Regex;
use serde::{Deserialize};
use serde_with::{serde_as, BorrowCow};

use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[derive(Deserialize, Debug)]
struct Envelope<'a> {
    #[serde(rename = "Payload-Metadata", borrow)]
    payload_metadata: PayloadMetadata<'a>,
}

#[derive(Deserialize, Debug)]
struct PayloadMetadata<'a> {
    #[serde(rename = "HTTP-Response-Metadata", borrow)]
    http_response_metadata: Option<HttpResponseMetadata<'a>>,
}

#[derive(Deserialize, Debug)]
struct HttpResponseMetadata<'a> {
    #[serde(rename = "HTML-Metadata", borrow)]
    html_metadata: Option<HtmlMetadata<'a>>,
}

#[derive(Deserialize, Debug)]
struct HtmlMetadata<'a> {
    #[serde(rename = "Links", borrow)]
    links: Option<Vec<Link<'a>>>,
}

#[serde_as]
#[derive(Deserialize, Debug)]
struct Link<'a> {
    #[serde_as(as = "Option<BorrowCow>")]
    url: Option<Cow<'a, str>>,
}

#[derive(Deserialize, Debug)]
struct Wat<'a> {
    #[serde(borrow)]
    Envelope: Envelope<'a>
}

fn main() {
    let reader = BufReader::new(stdin().lock());

    let mut buf = Vec::new();
    let mut decompressor = BufReader::with_capacity(65536, MultiGzDecoder::new(reader));

    let (send, recv) = bounded::<Vec<u8>>(512);

    let threads = (0..4).map(|_| {
        let recv = recv.clone();
        std::thread::spawn(move || {
            let prefilter = Regex::new(r"(?i)imgur").unwrap();
            while let Ok(content) = recv.recv() {
                if !prefilter.is_match(&content) {
                    continue;
                }
                let parse = serde_json::from_slice::<Wat>(&content);
                if parse.is_err() {
                    eprintln!("{:?} {}", parse, String::from_utf8(content.clone()).unwrap());
                    panic!();
                }
                let parse = parse.unwrap();
                if let Some(metadata) = parse.Envelope.payload_metadata.http_response_metadata {
                    if let Some(html_metadata) = metadata.html_metadata {
                        if let Some(links) = html_metadata.links {
                            for link in links {
                                let url = link.url;
                                if let Some(url) = url {
                                    if url.to_ascii_lowercase().contains("imgur") {
                                        println!("{}", url);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })
    }).collect::<Vec<_>>();

    while !decompressor.fill_buf().unwrap().is_empty() {
        let type_header = read_until_header(&mut decompressor, b"WARC-Type: ");
        let type_value = header_value(&type_header);

        let content_length_header = read_until_header(&mut decompressor, b"Content-Length: ");
        let content_length_value = header_value(&content_length_header);
        let content_length = std::str::from_utf8(content_length_value).unwrap().parse::<usize>().unwrap();

        decompressor.read_until(b'\n', &mut buf).unwrap();

        let mut content = vec![0u8; content_length];
        decompressor.read_exact(&mut content).unwrap();

        // dbg!(content_length);

        if type_value == b"metadata" {
            send.send(content).unwrap();
        }

        decompressor.read_until(b'\n', &mut buf).unwrap();
        decompressor.read_until(b'\n', &mut buf).unwrap();
        // dbg!(String::from_utf8(buf.to_vec()));
        buf.clear();
    }

    drop(send);

    for t in threads {
        t.join().unwrap();
    }
}

fn read_until_header(reader: &mut impl BufRead, header: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();
    
    loop {
        reader.read_until(b'\n', &mut buf).unwrap();
        // dbg!(String::from_utf8(buf.to_vec()));
        assert!(buf.len() > 2);
        if buf.starts_with(header) {
            return buf;
        }

        buf.clear();
    }
}

fn header_value(header: &[u8]) -> &[u8] {
    header.iter().position(|b| b == &b' ').map(|i| &header[i+1..header.len()-2]).unwrap()
}
