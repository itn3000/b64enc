use clap::Parser;
use tokio::io::{AsyncRead};
use std::pin::Pin;
use tokio::io::AsyncWrite;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    output: Option<String>,
    #[arg(short, long)]
    input: Option<String>,
    #[arg(short, long)]
    decode: bool,
    #[arg(long)]
    is_url: bool,
    #[arg(short, long)]
    line_length: Option<u16>,
    #[arg(short, long, default_value_t = 4096)]
    buffer_length: usize
}

enum StdinOrFile {
    Stdin(tokio::io::Stdin),
    File(tokio::fs::File),
}

enum StdoutOrFile {
    Stdout(tokio::io::Stdout),
    File(tokio::fs::File),
}

impl AsyncRead for StdinOrFile {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            StdinOrFile::Stdin(v) => {
                std::pin::Pin::new(v).poll_read(cx, buf)
            },
            StdinOrFile::File(v) => {
                std::pin::Pin::new(v).poll_read(cx, buf)
            }
        }
    }
}
impl AsyncWrite for StdoutOrFile {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        match self.get_mut() {
            StdoutOrFile::File(v) => Pin::new(v).poll_write(cx, buf),
            StdoutOrFile::Stdout(v) => Pin::new(v).poll_write(cx, buf)
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), std::io::Error>> {
        match self.get_mut() {
            StdoutOrFile::File(v) => Pin::new(v).poll_flush(cx),
            StdoutOrFile::Stdout(v) => Pin::new(v).poll_flush(cx)
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), std::io::Error>> {
        match self.get_mut() {
            StdoutOrFile::File(v) => Pin::new(v).poll_shutdown(cx),
            StdoutOrFile::Stdout(v) => Pin::new(v).poll_shutdown(cx)
        }
    }
}

mod b64enc;

use b64enc::encode;
use b64enc::decode;

async fn get_input(args: &Args) -> anyhow::Result<StdinOrFile> {
    match args.input.as_ref() {
        Some(v) => {
            let f = tokio::fs::File::open(v).await?;
            Ok(StdinOrFile::File(f))
        },
        None => Ok(StdinOrFile::Stdin(tokio::io::stdin()))
    }
}

async fn get_output(args: &Args) -> anyhow::Result<StdoutOrFile> {
    match args.output.as_ref() {
        Some(v) => {
            let f = tokio::fs::File::create(v).await?;
            Ok(StdoutOrFile::File(f))
        },
        None => Ok(StdoutOrFile::Stdout(tokio::io::stdout()))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Args::parse();
    let input = get_input(&opt).await?;
    let output = get_output(&opt).await?;
    if opt.decode {
        decode(input, output, opt.is_url, 
            if opt.buffer_length > 0 { opt.buffer_length } else { 4096 }).await?;
    } else {
        encode(input, output, 
            opt.line_length, 
            opt.is_url,
            if opt.buffer_length > 0 { opt.buffer_length } else { 4096 }).await?;
    }
    Ok(())
}
