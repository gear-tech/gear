// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use async_trait::async_trait;
use libp2p::{
    futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    request_response, StreamProtocol,
};
use parity_scale_codec::{Decode, DecodeAll, Encode};
use std::{io, marker::PhantomData};

pub(crate) struct ParityScaleCodec<Req, Resp>(PhantomData<(Req, Resp)>);

impl<Req, Resp> ParityScaleCodec<Req, Resp> {
    const MAX_REQUEST_SIZE: u64 = 1024 * 1024;
    const MAX_RESPONSE_SIZE: u64 = 10 * 1024 * 1024;
}

#[async_trait]
impl<Req, Resp> request_response::Codec for ParityScaleCodec<Req, Resp>
where
    Req: Send + Encode + Decode,
    Resp: Send + Encode + Decode,
{
    type Protocol = StreamProtocol;
    type Request = Req;
    type Response = Resp;

    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let mut vec = Vec::new();
        io.take(Self::MAX_REQUEST_SIZE)
            .read_to_end(&mut vec)
            .await?;
        Req::decode_all(&mut vec.as_slice()).map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    async fn read_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let mut vec = Vec::new();
        io.take(Self::MAX_RESPONSE_SIZE)
            .read_to_end(&mut vec)
            .await?;
        Resp::decode_all(&mut vec.as_slice()).map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    async fn write_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let vec = req.encode();
        io.write_all(&vec).await?;
        Ok(())
    }

    async fn write_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        res: Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let vec = res.encode();
        io.write_all(&vec).await?;
        Ok(())
    }
}

impl<Req, Resp> Default for ParityScaleCodec<Req, Resp> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<Req, Resp> Copy for ParityScaleCodec<Req, Resp> {}

impl<Req, Resp> Clone for ParityScaleCodec<Req, Resp> {
    fn clone(&self) -> Self {
        *self
    }
}

#[cfg(test)]
pub(crate) mod tests {
    pub fn init_logger() {
        let _ = env_logger::builder().is_test(true).try_init();
    }
}
