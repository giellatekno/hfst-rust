// NOTE: doctests are deliberately marked "compile_fail", because the code would
// rely on actually loading an FST, which we do not have any ability to do in a doctest
// that would run on docs.rs. (Unless we compiled a simple .hfstol and put as part
// of the source code.)

use super::HfstTransducerActor;
use crate::HfstTransducer;
use std::num::NonZeroUsize;

/// The builder for [`HfstTransducerActor`]. It takes three values:
/// - **transducer** (*required*). An [`crate::HfstTransducer`]. The transducer to use.
/// - **queue_size** (*required*) A [`std::num::NonZeroUsize`]. The size of the tokio mpsc queue.
///
/// ## Example
/// ```compile_fail
/// use hfst::transducer_actor::HfstTransducerActor;
/// let actor = HfstTransducerActor::builder()
///     .transducer(/* transducer */)
///     .queue_size(std::num::NonZeroUsize::new(100).unwrap())
///     .build();
/// ```
pub struct Builder<A, B> {
    transducer: A,
    queue_size: B,
}

// Beware: Custom implemented type state pattern builder below...

pub struct TransducerEmpty;
pub struct TransducerAdded(HfstTransducer);
pub struct QueueSizeEmpty;
pub struct QueueSizeAdded(NonZeroUsize);

pub type EmptyBuilder = Builder<TransducerEmpty, QueueSizeEmpty>;

impl Default for Builder<TransducerEmpty, QueueSizeEmpty> {
    fn default() -> Self {
        Self {
            transducer: TransducerEmpty,
            queue_size: QueueSizeEmpty,
        }
    }
}

#[doc(hidden)]
impl Builder<TransducerEmpty, QueueSizeEmpty> {
    pub fn transducer(self, tr: HfstTransducer) -> Builder<TransducerAdded, QueueSizeEmpty> {
        Builder {
            transducer: TransducerAdded(tr),
            queue_size: QueueSizeEmpty,
        }
    }

    pub fn queue_size(self, size: NonZeroUsize) -> Builder<TransducerEmpty, QueueSizeAdded> {
        Builder {
            transducer: TransducerEmpty,
            queue_size: QueueSizeAdded(size),
        }
    }
}

#[doc(hidden)]
impl Builder<TransducerAdded, QueueSizeEmpty> {
    pub fn queue_size(self, size: NonZeroUsize) -> Builder<TransducerAdded, QueueSizeAdded> {
        Builder {
            transducer: self.transducer,
            queue_size: QueueSizeAdded(size),
        }
    }
}

#[doc(hidden)]
impl Builder<TransducerEmpty, QueueSizeAdded> {
    pub fn transducer(self, tr: HfstTransducer) -> Builder<TransducerAdded, QueueSizeAdded> {
        Builder {
            transducer: TransducerAdded(tr),
            queue_size: self.queue_size,
        }
    }
}

#[doc(hidden)]
impl Builder<TransducerAdded, QueueSizeAdded> {
    pub fn build(self) -> HfstTransducerActor {
        let transducer = self.transducer.0;
        let queue_size = self.queue_size.0.get();
        HfstTransducerActor::new(transducer, queue_size)
    }
}
