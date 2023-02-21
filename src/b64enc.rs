use anyhow::Result;
use base64::engine::general_purpose;
use base64::Engine;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;

#[cfg(windows)]
const LINE_ENDING: &[u8] = &[0x0d, 0x0a];
#[cfg(not(windows))]
const LINE_ENDING: &[u8] = &[0x0a];

async fn write_encoded<TOut>(output: &mut TOut, prebytes: Vec<u8>, line_length: &Option<u16>, encoded: &[u8]) -> Result<Vec<u8>> 
    where TOut: 'static + AsyncWrite + std::marker::Unpin + std::marker::Send,
{
    let mut remaining_bytes = prebytes;
    match line_length.as_ref() {
        Some(v) => {
            let mut tmp: Vec<u8> = Vec::with_capacity(*v as usize);
            for b in remaining_bytes.iter().chain(encoded) {
                tmp.push(*b);
                if tmp.len() >= *v as usize {
                    output.write(tmp.as_ref()).await?;
                    output.write(LINE_ENDING).await?;
                    tmp.truncate(0);
                }
            }
            remaining_bytes = tmp;
        },
        None => {
            output.write(encoded).await?;
        }
    };
    Ok(remaining_bytes)
}

pub async fn encode<TIn, TOut>(mut input: TIn, mut output: TOut, line_length: Option<u16>, is_url: bool, buffer_length: usize) -> Result<()>
where
    TIn: 'static + AsyncRead + std::marker::Unpin + std::marker::Send,
    TOut: 'static + AsyncWrite + std::marker::Unpin + std::marker::Send,
{
    let (sender, mut recv) = tokio::sync::mpsc::channel::<Vec<u8>>(buffer_length);
    let fin = tokio::spawn(async move {
        let mut buf: Vec<u8> = Vec::with_capacity(buffer_length);
        buf.extend((0..buffer_length).map(|_| 1u8));
        loop {
            let bytesread = input.read(buf.as_mut_slice()).await?;
            if bytesread == 0 {
                break;
            }
            sender.send(Vec::from(&buf[0..bytesread])).await?;
        }
        Ok::<u32, anyhow::Error>(0)
    });
    let fout = tokio::spawn(async move {
        let mut buf: Vec<u8> = Vec::new();
        buf.reserve(buffer_length * 2);
        let mut remainingbytes: Vec<u8> = Vec::with_capacity(buffer_length);
        let mut encodedbytes = Vec::<u8>::with_capacity(buffer_length * 2);
        encodedbytes.extend((0..(buffer_length * 2)).map(|_| 0u8));
        loop {
            let rbuf = recv.recv().await;
            if let Some(mut data) = rbuf {
                // let r = output.write(&mut data).await;
                buf.append(&mut data);
                let x = buf.len() % 3;
                let byteswritten = if is_url {
                    // general_purpose::URL_SAFE.encode(&buf[0..buf.len() - x])
                    general_purpose::URL_SAFE.encode_slice(&buf[0..buf.len() - x], &mut encodedbytes)
                } else {
                    general_purpose::STANDARD.encode_slice(&buf[0..buf.len() - x], &mut encodedbytes)
                }?;
                let tmp = if x != 0 { Vec::from(&buf[(buf.len() - x)..]) } else { Vec::new() };
                remainingbytes = write_encoded(&mut output, remainingbytes, &line_length, &encodedbytes[0..byteswritten]).await?;
                buf.truncate(0);
                buf.extend(tmp.iter());
            } else {
                break;
            }
        }
        if buf.len() != 0 {
            let byteswritten = if is_url {
                general_purpose::URL_SAFE.encode_slice(&buf, &mut encodedbytes)
            } else {
                general_purpose::STANDARD.encode_slice(&buf, &mut encodedbytes)
            }?;
            remainingbytes = write_encoded(&mut output, remainingbytes, &line_length, &encodedbytes[0..byteswritten]).await?;
            if remainingbytes.len() != 0 {
                output.write(&remainingbytes).await?;
            }
        }
        Ok::<u32, anyhow::Error>(0)
    });
    let (rin, rout) = tokio::join!(fin, fout);
    let _ = (rin?)?;
    let _ = (rout?)?;
    Ok(())
}

pub async fn decode<TIn, TOut>(mut input: TIn, mut output: TOut, is_url: bool, buffer_length: usize) -> Result<()>
where
    TIn: 'static + AsyncRead + std::marker::Unpin + std::marker::Send,
    TOut: 'static + AsyncWrite + std::marker::Unpin + std::marker::Send,
{
    let (sender, mut recv) = tokio::sync::mpsc::channel::<String>(buffer_length);
    let fin = tokio::spawn(async move {
        let mut buf: Vec<u8> = Vec::with_capacity(buffer_length);
        buf.extend((0..buffer_length).map(|_| 0u8));
        loop {
            let bytesread = input.read(buf.as_mut_slice()).await?;
            if bytesread == 0 {
                break;
            }
            let s = String::from_utf8(Vec::from(&buf[0..bytesread]))?;
            sender.send(s).await?;
        }
        Ok::<u32, anyhow::Error>(0)
    });
    let fout = tokio::spawn(async move {
        let mut buf = String::with_capacity(buffer_length * 2);
        let mut decoded: Vec<u8> = Vec::with_capacity(buffer_length * 2);
        decoded.extend((0..buffer_length).map(|_| 0u8));
        loop {
            let rbuf = recv.recv().await;
            if let Some(data) = rbuf {
                // let r = output.write(&mut data).await;
                buf.extend(data.chars().filter(|c| (*c != ' ') && (*c != '\n') && (*c != '\r')));
                let x = if buf.len() >= 4 { buf.len() % 4 } else { 0 };
                let remaining: Vec<char> = buf.chars().skip(buf.len() - x).collect();
                let byteswritten = if is_url {
                    general_purpose::URL_SAFE.decode_slice(&buf[0..buf.len() - x], &mut decoded)
                } else {
                    general_purpose::STANDARD.decode_slice(&buf[0..buf.len() - x], &mut decoded)
                }?;
                output.write(&decoded[0..byteswritten]).await?;
                buf.truncate(0);
                buf.extend(remaining.iter());
            } else {
                break;
            }
        }
        if buf.len() != 0 {
            let byteswritten = if is_url {
                general_purpose::URL_SAFE.decode_slice(&buf, &mut decoded)
            } else {
                general_purpose::STANDARD.decode_slice(&buf, &mut decoded)
            }?;
            output.write(&decoded[0..byteswritten]).await?;
        }
        Ok::<u32, anyhow::Error>(0)
    });
    let (rin, rout) = tokio::join!(fin, fout);
    let _ = (rin?)?;
    let _ = (rout?)?;
    Ok(())
}
