//! A [tokio](https://docs.rs/tokio) *actor* for doing lookups from multiple tokio tasks.
//!
//! The main [`crate::HfstTransducer`] is *not* thread-safe, and only supports
//! doing one lookup at a time. This module defines a tokio *actor* that tasks
//! can send lookup requests to in parallel, and return back results.
//!
//! Lookup request messages are sent to the actor, and the actor simply runs an
//! infinite loop where it pulls off lookup requests, one by one. It does the
//! lookup, and sends back the replies in a *oneshot* channel.
//!
//! # Example
//! ```rust
//! use std::sync::Arc;
//! use hfst::transducer_actor::{LookupResult, HfstTransducerActor};
//!
//! // First, build yourself a Transducer
//! let transducer = /* some transducer */();
//!
//! // Then, build an actor that takes ownership of this transducer.
//! let actor = HfstTransducerActor::builder()
//!     .transducer(transducer)
//!     .queue_size(std::num::NonZeroUsize::new(100).unwrap())
//!     .timings(true)
//!     .build();
//!
//! // Put the actor in an Arc, so it can be shared
//! let actor = Arc::new(actor);
//!
//! let tasks: Vec<_> = (0..9).map(|_| tokio::task::spawn({
//!     // Get a new reference to the actor.
//!     let actor = Arc::clone(&actor);
//!     // ...The new reference is moved in.
//!     async move {
//!         let lookup = actor.lookup("viessu").await.expect("lookup did not error");
//!         let LookupResults { results, .. } = lookup;
//!         println!("{}", results.join("\n"));
//!     }
//! })).collect();
//!
//! for task in tasks {
//!     task.await.expect("task did not panic");
//! }
//! ```

use std::time::{Duration, Instant};

use tokio::sync::{mpsc, oneshot};

use crate::{HfstInputStream, HfstTransducer};

/// A running HfstTransducer actor.
pub struct HfstTransducerActor {
    jh: tokio::task::JoinHandle<HfstTransducer>,
    tx: mpsc::Sender<LookupMessage>,
}

/// The result we get back from `HfstTransducerActor::lookup()`.
pub struct LookupResults {
    /// The actual results: The string, and the weight.
    pub results: Vec<(String, f32)>,

    /// We did wait before we entered the queue, and if so, for how long?
    pub before_queue: Waited,

    /// Did we wait *in* the queue, and if so, for how long?
    pub in_queue: Waited,

    /// How long the actual lookup took
    pub lookup_duration: Duration,

    /// How long it took before the result came back
    pub result_duration: Duration,
}

#[derive(Debug, thiserror::Error)]
pub enum LookupError {
    #[error("channel to actor was closed")]
    ChannelClosed,
}

/// Did we wait? If so, for how long?
pub enum Waited {
    Yes(Duration),
    No,
}

/// Message that is sent to the lookup actor from the many clients.
enum LookupMessage {
    Lookup(String, oneshot::Sender<LookupReply>),

    /// Message to quit the actor
    Quit,
}

/// Internal Reply message that is sent back from the actor to `HfstTransducerActor::lookup()`
#[derive(Debug)]
struct LookupReply {
    results: Vec<(String, f32)>,
    lookup_duration: Duration,
}

mod builder {
    use super::HfstTransducerActor;
    use crate::HfstTransducer;
    use std::num::NonZeroUsize;

    /// The builder for [`HfstTransducerActor`]. It takes three values:
    /// - **transducer** (*required*). An [`crate::HfstTransducer`]. The transducer to use.
    /// - **queue_size** (*required*) A [`std::num::NonZeroUsize`]. The size of the tokio mpsc queue.
    /// - **timings** (*optional*), a [`bool`]. Whether or not to return timings in lookups.
    ///
    /// ## Example
    /// ```
    /// let actor = HfstTransducerActor::builder()
    ///     .transducer(/* transducer */)
    ///     .queue_size(std::num::NonZeroUsize::new(100).unwrap())
    ///     .timings(true)
    ///     .build();
    /// ```
    pub struct Builder<A, B, C> {
        transducer: A,
        queue_size: B,
        timings: C,
    }

    // Beware: Custom implemented type state pattern builder below...

    pub struct TransducerEmpty;
    pub struct TransducerAdded(HfstTransducer);
    pub struct QueueSizeEmpty;
    pub struct QueueSizeAdded(NonZeroUsize);
    pub struct TimingsEmpty;
    pub struct TimingsAdded(bool);

    pub type EmptyBuilder = Builder<TransducerEmpty, QueueSizeEmpty, TimingsEmpty>;

    impl Default for Builder<TransducerEmpty, QueueSizeEmpty, TimingsEmpty> {
        fn default() -> Self {
            Self {
                transducer: TransducerEmpty,
                queue_size: QueueSizeEmpty,
                timings: TimingsEmpty,
            }
        }
    }

    #[doc(hidden)]
    impl Builder<TransducerEmpty, QueueSizeEmpty, TimingsEmpty> {
        pub fn transducer(
            self,
            tr: HfstTransducer,
        ) -> Builder<TransducerAdded, QueueSizeEmpty, TimingsEmpty> {
            Builder {
                transducer: TransducerAdded(tr),
                queue_size: QueueSizeEmpty,
                timings: TimingsEmpty,
            }
        }

        pub fn queue_size(
            self,
            size: NonZeroUsize,
        ) -> Builder<TransducerEmpty, QueueSizeAdded, TimingsEmpty> {
            Builder {
                transducer: TransducerEmpty,
                queue_size: QueueSizeAdded(size),
                timings: TimingsEmpty,
            }
        }

        pub fn timings(
            self,
            enabled: bool,
        ) -> Builder<TransducerEmpty, QueueSizeEmpty, TimingsAdded> {
            Builder {
                transducer: TransducerEmpty,
                queue_size: QueueSizeEmpty,
                timings: TimingsAdded(enabled),
            }
        }
    }

    // === (1, 0, 0)
    #[doc(hidden)]
    impl Builder<TransducerAdded, QueueSizeEmpty, TimingsEmpty> {
        pub fn queue_size(
            self,
            size: NonZeroUsize,
        ) -> Builder<TransducerAdded, QueueSizeAdded, TimingsEmpty> {
            Builder {
                transducer: self.transducer,
                queue_size: QueueSizeAdded(size),
                timings: TimingsEmpty,
            }
        }

        pub fn timings(
            self,
            enabled: bool,
        ) -> Builder<TransducerAdded, QueueSizeEmpty, TimingsAdded> {
            Builder {
                transducer: self.transducer,
                queue_size: QueueSizeEmpty,
                timings: TimingsAdded(enabled),
            }
        }
    }

    // === (0, 1, 0)
    #[doc(hidden)]
    impl Builder<TransducerEmpty, QueueSizeAdded, TimingsEmpty> {
        pub fn transducer(
            self,
            tr: HfstTransducer,
        ) -> Builder<TransducerAdded, QueueSizeAdded, TimingsEmpty> {
            Builder {
                transducer: TransducerAdded(tr),
                queue_size: self.queue_size,
                timings: TimingsEmpty,
            }
        }

        pub fn timings(
            self,
            enabled: bool,
        ) -> Builder<TransducerEmpty, QueueSizeAdded, TimingsAdded> {
            Builder {
                transducer: TransducerEmpty,
                queue_size: self.queue_size,
                timings: TimingsAdded(enabled),
            }
        }
    }

    // === (0, 0, 1)
    #[doc(hidden)]
    impl Builder<TransducerEmpty, QueueSizeEmpty, TimingsAdded> {
        pub fn transducer(
            self,
            tr: HfstTransducer,
        ) -> Builder<TransducerAdded, QueueSizeEmpty, TimingsAdded> {
            Builder {
                transducer: TransducerAdded(tr),
                queue_size: QueueSizeEmpty,
                timings: self.timings,
            }
        }

        pub fn queue_size(
            self,
            size: NonZeroUsize,
        ) -> Builder<TransducerEmpty, QueueSizeAdded, TimingsAdded> {
            Builder {
                transducer: TransducerEmpty,
                queue_size: QueueSizeAdded(size),
                timings: self.timings,
            }
        }
    }

    #[doc(hidden)]
    impl Builder<TransducerAdded, QueueSizeAdded, TimingsEmpty> {
        pub fn timings(
            self,
            enabled: bool,
        ) -> Builder<TransducerAdded, QueueSizeAdded, TimingsAdded> {
            Builder {
                transducer: self.transducer,
                queue_size: self.queue_size,
                timings: TimingsAdded(enabled),
            }
        }

        pub fn build(self) -> HfstTransducerActor {
            let transducer = self.transducer.0;
            let queue_size = self.queue_size.0.get();
            HfstTransducerActor::new(transducer, queue_size)
        }
    }

    #[doc(hidden)]
    impl Builder<TransducerAdded, QueueSizeAdded, TimingsAdded> {
        pub fn build(self) -> HfstTransducerActor {
            let transducer = self.transducer.0;
            let queue_size = self.queue_size.0.get();
            HfstTransducerActor::new(transducer, queue_size)
        }
    }
}

impl HfstTransducerActor {
    /// Create a new `HfstTransducerActor` through this easy-to-use [`builder::Builder`].
    pub fn builder() -> builder::EmptyBuilder {
        builder::Builder::default()
    }

    fn new(transducer: HfstTransducer, queue_size: usize) -> HfstTransducerActor {
        let (tx, mut rx) = mpsc::channel(queue_size);

        let jh = tokio::task::spawn(async move {
            while let Some(msg) = rx.recv().await {
                match msg {
                    LookupMessage::Lookup(input, result_tx) => {
                        let t0 = Instant::now();
                        let results: Vec<_> = transducer.lookup(&input).into_iter().collect();
                        let lookup_duration = t0.elapsed();
                        let reply_message = LookupReply {
                            results,
                            lookup_duration,
                        };
                        result_tx
                            .send(reply_message)
                            .expect("reciever didn't hang up");
                    }
                    LookupMessage::Quit => break,
                }
            }
            transducer
        });

        HfstTransducerActor { jh, tx }
    }

    /// Look up a value in the transducer.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use hfst::transducer_actor::LookupResults;
    ///
    /// let actor = /* ... */();
    /// actor.lookup("input").await.unwrap();
    /// let mut join_handles = vec![];
    /// for _ in 0..10 {
    ///     join_handles.push(tokio::task::spawn({
    ///         let actor = Arc::clone(&actor);
    ///         async move {
    ///             let LookupResults { results, .. } = actor.lookup("input").await;
    ///             results.for_each(|result| println!("{result}"));
    ///         }
    ///     }));
    /// }
    /// for join_handle in join_handles {
    ///     join_handle.await.expect("task did not panic");
    /// }
    /// ```
    pub async fn lookup(&self, input: &str) -> Result<LookupResults, LookupError> {
        if self.tx.is_closed() {
            return Err(LookupError::ChannelClosed);
        }

        let tx = self.tx.clone();
        let (os_tx, os_rx) = oneshot::channel();
        let message = LookupMessage::Lookup(input.into(), os_tx);
        let before_queue = match tx.try_send(message) {
            Ok(()) => Waited::No,
            Err(mpsc::error::TrySendError::Closed(_message)) => {
                return Err(LookupError::ChannelClosed);
            }
            Err(mpsc::error::TrySendError::Full(message)) => {
                let t0 = Instant::now();
                match tx.reserve().await {
                    Ok(permit) => {
                        let before_queue = Waited::Yes(t0.elapsed());
                        permit.send(message);
                        before_queue
                    }
                    Err(_) => {
                        return Err(LookupError::ChannelClosed);
                    }
                }
            }
        };

        // Message has been sent here into the queue here. We don't know at what position
        // in the queue it entered into, or if there even was a queue at all.
        let t0 = Instant::now();
        let lookup_reply = os_rx.await.expect("channel was not closed in transit");
        let result_duration = t0.elapsed();

        let LookupReply {
            results,
            lookup_duration,
        } = lookup_reply;

        // Here we have to calculate a bit.
        // We have result duration, which is the entire time from when the message
        // was accepted into the queue, and we also have the actual time it took to look
        // up the value, from the actor, so, we can calculate how long we waited in
        // the queue.
        let in_queue = Waited::Yes(result_duration - lookup_duration);

        Ok(LookupResults {
            results,
            before_queue,
            in_queue,
            result_duration,
            lookup_duration,
        })
    }

    /// Stop the actor. Returns the ownership of the underlying [`HfstTransducer`] back
    /// the caller.
    pub async fn stop(self) -> HfstTransducer {
        let HfstTransducerActor { tx, jh } = self;
        let transducer = jh.await.expect("actor did not panic");
        tx.send(LookupMessage::Quit)
            .await
            .expect("channel was not already closed");
        transducer
    }
}
