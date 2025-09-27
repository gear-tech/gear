#![cfg(test)]

use std::{
    io,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use demo_web2_verifier::{Direction, TlsRecord};

pub struct TlsTap<IO> {
    inner: IO,
    in_buf: Vec<u8>,
    out_buf: Vec<u8>,
    server_records: Vec<TlsRecord>,
    client_records: Vec<TlsRecord>,
    c2s_seq: u64,
    s2c_seq: u64,
}

impl<IO> TlsTap<IO> {
    pub fn new(inner: IO) -> Self {
        Self {
            inner,
            in_buf: Vec::new(),
            out_buf: Vec::new(),
            server_records: Vec::new(),
            client_records: Vec::new(),
            c2s_seq: 0,
            s2c_seq: 0,
        }
    }

    pub fn into_records(self) -> (Vec<TlsRecord>, Vec<TlsRecord>) {
        (self.client_records, self.server_records)
    }
}

impl<IO: AsyncRead + AsyncWrite + Unpin> AsyncRead for TlsTap<IO> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let filled_before = buf.filled().len();
        let r = std::pin::Pin::new(&mut self.inner).poll_read(cx, buf);

        if let Poll::Ready(Ok(())) = r {
            let filled_after = buf.filled().len();
            if filled_after > filled_before {
                let newly = &buf.filled()[filled_before..filled_after];
                self.in_buf.extend_from_slice(newly);

                let (new_seq, recs) =
                    drain_tls_records(self.s2c_seq, &mut self.in_buf, Direction::Server);

                self.s2c_seq = new_seq;
                self.server_records.extend(recs);
            }
        }
        r
    }
}

impl<IO: AsyncRead + AsyncWrite + Unpin> AsyncWrite for TlsTap<IO> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        match std::pin::Pin::new(&mut self.inner).poll_write(cx, buf) {
            Poll::Ready(Ok(n)) => {
                if n > 0 {
                    self.out_buf.extend_from_slice(&buf[..n]);

                    let (new_seq, recs) =
                        drain_tls_records(self.c2s_seq, &mut self.out_buf, Direction::Client);
                    self.c2s_seq = new_seq;
                    self.client_records.extend(recs);
                }
                Poll::Ready(Ok(n))
            }
            other => other,
        }
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_write_vectored(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<Result<usize, io::Error>> {
        match std::pin::Pin::new(&mut self.inner).poll_write_vectored(cx, bufs) {
            Poll::Ready(Ok(n)) => {
                if n > 0 {
                    // собрать первые n байт из iovecs
                    let mut left = n;
                    for s in bufs {
                        if left == 0 {
                            break;
                        }
                        let take = left.min(s.len());
                        self.out_buf.extend_from_slice(&s[..take]);
                        left -= take;
                    }

                    let (new_seq, recs) =
                        drain_tls_records(self.c2s_seq, &mut self.out_buf, Direction::Client);
                    self.c2s_seq = new_seq;
                    self.client_records.extend(recs);
                }
                Poll::Ready(Ok(n))
            }
            other => other,
        }
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

fn drain_tls_records(
    mut seq: u64,
    buf: &mut Vec<u8>,
    direction: Direction,
) -> (u64, Vec<TlsRecord>) {
    let mut out = Vec::new();
    let mut offset = 0usize;

    while buf.len().saturating_sub(offset) >= 5 {
        let ct = buf[offset];
        let ver = u16::from_be_bytes([buf[offset + 1], buf[offset + 2]]);
        let len = u16::from_be_bytes([buf[offset + 3], buf[offset + 4]]) as usize;

        if len > (1 << 14) + 256 {
            offset += 1;
            continue;
        }
        if buf.len().saturating_sub(offset) < 5 + len {
            break;
        }
        if !matches!(ct, 20 | 21 | 22 | 23) || !matches!(ver, 0x0301 | 0x0302 | 0x0303) {
            offset += 1;
            continue;
        }

        let rec_bytes = &buf[offset..offset + 5 + len];

        out.push(TlsRecord {
            direction,
            content_type: ct.into(),
            version: ver,
            length: len as u16,
            seq,
            bytes: rec_bytes.to_vec(),
        });

        seq = seq.wrapping_add(1);
        offset += 5 + len;
    }

    if offset > 0 {
        buf.drain(..offset);
    }

    (seq, out)
}
