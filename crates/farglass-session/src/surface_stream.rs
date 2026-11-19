//! The surface channel: the tile stream of one surface (F025).
//!
//! A watch of the surface gives each commit. The commits wait in one slot:
//! a new commit adds its damage to the one that waits, thus a slow encoder
//! sends the newest pixels only. The encoder runs on a blocking thread.

use std::sync::{Arc, Mutex, PoisonError};

use farglass_proto::{ChannelId, Frame, MAX_PAYLOAD_LEN};
use farglass_surface::tiles::{self, Encoder, Image};
use tokio::sync::{Notify, mpsc};
use tokio::task::JoinHandle;

use crate::surfaces::{FrameUpdate, Surfaces, WatchGuard};

/// One open surface channel. Dropping it stops the encoder and the watch.
#[derive(Debug)]
pub struct SurfaceStream {
    _watch: WatchGuard,
    task: JoinHandle<()>,
}

impl Drop for SurfaceStream {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// The work of one step of the encoder.
enum Job {
    /// A commit, or several commits with their damage.
    Frame(FrameUpdate),
    /// The lossless pass of the lossy tiles.
    Refine,
}

/// Adds `update` to the update that waits.
fn merge(waiting: &mut Option<FrameUpdate>, update: FrameUpdate) {
    match waiting {
        Some(old) if (old.width, old.height) == (update.width, update.height) => {
            old.pixels = update.pixels;
            old.damage.extend(update.damage);
        }
        _ => *waiting = Some(update),
    }
}

impl SurfaceStream {
    /// Starts the stream of `surface` on `channel`. The data frames go to `frames`.
    #[must_use]
    pub fn open(
        surfaces: &Arc<Surfaces>,
        surface: u32,
        channel: ChannelId,
        frames: mpsc::UnboundedSender<Frame>,
    ) -> Self {
        let waiting: Arc<Mutex<Option<FrameUpdate>>> = Arc::new(Mutex::new(None));
        let notify = Arc::new(Notify::new());
        let watch = {
            let waiting = waiting.clone();
            let notify = notify.clone();
            surfaces.watch(surface, move |update| {
                merge(
                    &mut waiting.lock().unwrap_or_else(PoisonError::into_inner),
                    update,
                );
                notify.notify_one();
            })
        };
        let task = tokio::spawn(async move {
            if let Ok(hello) = Frame::data(channel, tiles::hello(farglass_surface::cache::SLOTS)) {
                let _ = frames.send(hello);
            }
            let mut encoder = Encoder::new();
            loop {
                // The lossless pass of the lossy tiles comes when they did not change for a time (F028).
                let refine_at = encoder.next_refine();
                let sleep = async {
                    match refine_at {
                        Some(at) => {
                            tokio::time::sleep_until(tokio::time::Instant::from_std(at)).await
                        }
                        None => std::future::pending().await,
                    }
                };
                let job: Job = tokio::select! {
                    () = notify.notified() => match waiting.lock().unwrap_or_else(PoisonError::into_inner).take() {
                        Some(update) => Job::Frame(update),
                        None => continue,
                    },
                    () = sleep => Job::Refine,
                };
                let Ok((back, encoded)) = tokio::task::spawn_blocking(move || {
                    let encoded = match job {
                        Job::Frame(update) => {
                            let image = Image {
                                width: update.width,
                                height: update.height,
                                pixels: &update.pixels,
                            };
                            encoder.encode(&image, &update.damage)
                        }
                        Job::Refine => encoder.refine(std::time::Instant::now()),
                    };
                    (encoder, encoded)
                })
                .await
                else {
                    return;
                };
                encoder = back;
                let Some(encoded) = encoded else { continue };
                tracing::trace!(surface, seq = encoded.seq, stats = ?encoded.stats, "surface frame");
                for chunk in encoded.bytes.chunks(MAX_PAYLOAD_LEN) {
                    if let Ok(frame) = Frame::data(channel, chunk.to_vec())
                        && frames.send(frame).is_err()
                    {
                        return;
                    }
                }
            }
        });
        Self {
            _watch: watch,
            task,
        }
    }
}
