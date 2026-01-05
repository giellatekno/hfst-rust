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
//! ```compile_fail
//! use std::sync::Arc;
//! use hfst::transducer_actor::{LookupResults, HfstTransducerActor};
//!
//! // First, build yourself a Transducer
//! let transducer = /* some transducer */();
//!
//! // Then, build an actor that takes ownership of this transducer.
//! let actor = HfstTransducerActor::builder()
//!     .transducer(transducer)
//!     .queue_size(std::num::NonZeroUsize::new(100).unwrap())
//!     .build();
//!
//! // Put the actor in an Arc, so it can be shared
//! let actor = Arc::new(actor);
//!
//! # async fn main() {
//! let tasks: Vec<_> = (0..9).map(|_| tokio::task::spawn({
//!     // Get a new reference to the actor.
//!     let actor = Arc::clone(&actor);
//!     // ...The new reference is moved in.
//!     async move || {
//!         let lookup = actor.lookup("viessu").await.expect("lookup did not error");
//!         let LookupResults { results, .. } = lookup;
//!         println!("{}", results.join("\n"));
//!     }
//! }).collect();
//!
//! for task in tasks {
//!     task.await.expect("task did not panic");
//! }
//! # }
//! ```

mod builder;

use std::time::{Duration, Instant};

use tokio::sync::{mpsc, oneshot};

/// A running HfstTransducer actor.
pub struct HfstTransducerActor {
    jh: tokio::task::JoinHandle<crate::HfstTransducer>,
    tx: mpsc::Sender<LookupMessage>,
}

/// The result we get back from `HfstTransducerActor::lookup()`.
pub struct LookupResults {
    /// The actual results: The string, and the weight.
    pub results: Vec<(String, f32)>,

    /// How long the various parts took
    pub timings: LookupResultTimings,
}

pub struct LookupManyResults {
    /// The actual results: The input string, and each of the results and weight for each
    /// input string.
    pub results: Vec<(String, Vec<(String, f32)>)>,

    /// How long the various parts took
    pub timings: LookupResultTimings,
}

pub struct LookupResultTimings {
    /// Did we wait *before we entered* the queue, and if so, for how long? `None` if
    /// we did not have to wait, and `Some(Duration)` if we did.
    pub before_queue: Option<Duration>,

    /// Did we wait *in* the queue, and if so, for how long? `None` if we did not wait,
    /// and `Some(Duration)` if we did.
    pub in_queue: Option<Duration>,

    /// How long the actual lookup took.
    pub lookup_duration: Duration,

    /// How long it took before the result came back.
    pub result_duration: Duration,
}

#[derive(Debug, thiserror::Error)]
pub enum LookupError {
    /// The channel was closed.
    #[error("channel to actor was closed")]
    ChannelClosed,
    /// Failed to send a `LookupMessage::Lookup` message on the channel.
    #[error("failed to send a LookupMessage::Lookup on channel")]
    TrySendError(String),
    /// Failed to send a `LookupMessage::LookupMany` message on the channel.
    #[error("failed to send a LookupMessage::LookupMany on channel")]
    TrySendManyError(Vec<String>),
}

/// Message that is sent to the lookup actor from the many clients.
enum LookupMessage {
    Lookup(String, oneshot::Sender<LookupReply>),
    LookupMany(Vec<String>, oneshot::Sender<LookupManyReply>),

    /// Message to quit the actor
    Quit,
}

/// Internal Reply message that is sent back from the actor to `HfstTransducerActor::lookup()`
#[derive(Debug)]
struct LookupReply {
    results: Vec<(String, f32)>,
    lookup_duration: Duration,
}

/// Internal Reply message that is sent back from the actor to
/// `HfstTransducerActor::lookup_many()`
#[derive(Debug)]
struct LookupManyReply {
    results: Vec<(String, Vec<(String, f32)>)>,
    lookup_duration: Duration,
}

/// SAFETY: ...
/// Needed because we want to store actors as the value type in a HashMap.
unsafe impl Send for HfstTransducerActor {}
unsafe impl Sync for HfstTransducerActor {}

impl HfstTransducerActor {
    /// Create a new `HfstTransducerActor` through this easy-to-use [`builder::Builder`].
    pub fn builder() -> builder::EmptyBuilder {
        builder::Builder::default()
    }

    fn new(transducer: crate::HfstTransducer, queue_size: usize) -> HfstTransducerActor {
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
                            .expect("oneshot reciever didn't hang up");
                    }
                    LookupMessage::LookupMany(inputs, result_tx) => {
                        let t0 = Instant::now();
                        let results: Vec<_> = inputs
                            .into_iter()
                            .map(|input| {
                                (input.to_string(), transducer.lookup(&input).into_iter().collect())
                            })
                            .collect();
                        let lookup_duration = t0.elapsed();
                        result_tx.send(LookupManyReply {
                            results,
                            lookup_duration,
                        }).expect("oneshot reciever didn't hang up");
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
    /// ```compile_fail
    /// use std::sync::Arc;
    /// use hfst::transducer_actor::{HfstTransducerActor, LookupResults};
    ///
    /// let actor = /* ... */();
    /// actor.lookup("input").await.unwrap();
    /// let mut join_handles = vec![];
    /// for _ in 0..10 {
    ///     join_handles.push(tokio::task::spawn({
    ///         let actor: Arc<HfstTransducerActor> = Arc::clone(&actor);
    ///         async move {
    ///             let LookupResults { results, .. } = actor.lookup("input").await.expect("lookup succeeded");
    ///             results.into_iter().for_each(|(result, _weight)| println!("{result}"));
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
            Ok(()) => None,
            Err(mpsc::error::TrySendError::Closed(_message)) => {
                return Err(LookupError::ChannelClosed);
            }
            Err(mpsc::error::TrySendError::Full(message)) => {
                let t0 = Instant::now();
                match tx.reserve().await {
                    Ok(permit) => {
                        let before_queue = Some(t0.elapsed());
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
        let in_queue = Some(result_duration - lookup_duration);

        Ok(LookupResults {
            results,
            timings: LookupResultTimings {
                before_queue,
                in_queue,
                result_duration,
                lookup_duration,
            },
        })
    }

    pub async fn lookup_many(&self, inputs: Vec<String>) -> Result<LookupManyResults, LookupError> {
        if self.tx.is_closed() {
            return Err(LookupError::ChannelClosed);
        }

        let tx = self.tx.clone();
        let (os_tx, os_rx) = oneshot::channel();
        let message = LookupMessage::LookupMany(inputs, os_tx);
        let before_queue = match tx.try_send(message) {
            Ok(()) => None,
            Err(mpsc::error::TrySendError::Closed(message)) => {
                let LookupMessage::LookupMany(inputs, _) = message else {
                    unreachable!("the message is a LookupMany message");
                };
                return Err(LookupError::TrySendManyError(inputs));
            }
            Err(mpsc::error::TrySendError::Full(message)) => {
                let t0 = Instant::now();
                match tx.reserve().await {
                    Ok(permit) => {
                        let before_queue = Some(t0.elapsed());
                        permit.send(message);
                        before_queue
                    }
                    Err(_) => {
                        let LookupMessage::LookupMany(inputs, _) = message else {
                            unreachable!("the message is a LookupMany message");
                        };
                        return Err(LookupError::TrySendManyError(inputs));
                    }
                }
            }
        };

        let t0 = Instant::now();
        let lookup_reply = os_rx.await.map_err(|_| LookupError::ChannelClosed)?;
        let result_duration = t0.elapsed();

        let LookupManyReply {
            results,
            lookup_duration,
        } = lookup_reply;

        let in_queue = Some(result_duration - lookup_duration);

        Ok(LookupManyResults {
            results,
            timings: LookupResultTimings {
                before_queue,
                in_queue,
                result_duration,
                lookup_duration,
            },
        })
    }

    /// Stop the actor. Returns the ownership of the underlying [`crate::HfstTransducer`]
    /// back the caller.
    pub async fn stop(self) -> crate::HfstTransducer {
        let HfstTransducerActor { tx, jh } = self;
        tx.send(LookupMessage::Quit)
            .await
            .expect("channel was not already closed");
        let transducer = jh.await.expect("actor did not panic");
        transducer
    }
}
