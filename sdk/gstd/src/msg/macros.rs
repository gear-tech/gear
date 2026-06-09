// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

macro_rules! impl_futures {
    ($f:ident, $r:ty, |$fut:ident, $cx:ident| => { $p:expr }) => {
        impl_futures!($f, $r, ($fut, $cx), $p, );
    };
    ($f:ident, $g: tt, $r:ty, |$fut:ident, $cx:ident| => { $p:expr }) => {
        impl_futures!($f, $r, ($fut, $cx), $p, $g);
    };
    ($f:ident, $r:ty, ($fut:ident, $cx:ident), $p:expr, $($g:tt)?) => {
        impl $( <$g: Decode> )? FusedFuture for $f $( < $g > )? {
            fn is_terminated(&self) -> bool {
                !signals().waits_for(self.waiting_reply_to)
            }
        }

        impl $( <$g: Decode> )? Future for $f $( < $g > )? {
            type Output = Result<$r>;

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let $fut = &mut self;
                let $cx = cx;

                $p
            }
        }

        impl $( <$g: Decode> )? $f $( < $g > )? {
            /// Postpone handling for a maximum amount of blocks that could be paid, that
            /// doesn't exceed a given duration.
            pub fn up_to(self, duration: Option<u32>) -> Result<Self> {
                async_runtime::locks().lock(
                    crate::msg::id(),
                    self.waiting_reply_to,
                    Lock::up_to(duration.unwrap_or(Config::wait_up_to()))?,
                );

                Ok(self)
            }

            /// Postpone handling for a given specific amount of blocks.
            pub fn exactly(self, duration: Option<u32>) -> Result<Self> {
                async_runtime::locks().lock(
                    crate::msg::id(),
                    self.waiting_reply_to,
                    Lock::exactly(duration.unwrap_or(Config::wait_for()))?,
                );

                Ok(self)
            }

            /// Execute a function when the reply is received.
            ///
            /// This callback will be executed in reply context and consume reply gas, so
            /// adequate `reply_deposit` should be supplied in `*_for_reply` call
            /// that comes before this. Note that the hook will still be executed on reply
            /// even after original future resolves in timeout.
            ///
            /// # Examples
            ///
            /// Send message to echo program and wait for reply.
            ///
            /// ```
            /// use gstd::{ActorId, msg, debug};
            ///
            /// #[gstd::async_main]
            /// async fn main() {
            ///     let dest = ActorId::from(1); // Replace with correct actor id
            ///
            ///     msg::send_bytes_for_reply(dest, b"PING", 0, 1_000_000)
            ///         .expect("Unable to send")
            ///         .handle_reply(|| {
            ///             debug!("reply code: {:?}", msg::reply_code());
            ///
            ///             if msg::load_bytes().unwrap_or_default() == b"PONG" {
            ///                debug!("successfully received pong");
            ///             }
            ///         })
            ///         .expect("Error setting reply hook")
            ///         .await
            ///         .expect("Received error");
            /// }
            /// # fn main() {}
            /// ```
            ///
            /// # Panics
            ///
            /// Panics if this is called second time.
            #[cfg(not(feature = "ethexe"))]
            pub fn handle_reply<F: FnOnce() + 'static>(self, f: F) -> Result<Self> {
                if self.reply_deposit == 0 {
                    return Err(Error::Gstd(crate::errors::UsageError::ZeroReplyDeposit));
                }
                async_runtime::reply_hooks().register(self.waiting_reply_to.clone(), f);

                Ok(self)
            }
        }
    };
}
