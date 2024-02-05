use crate::{CallbackResult, ClipboardHandler};

use std::io;
use std::sync::mpsc::{self, SyncSender, Receiver, sync_channel};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

use arboard::Clipboard;


///Shutdown channel
///
///On drop requests shutdown to gracefully close clipboard listener as soon as possible.
pub struct Shutdown {
    sender: SyncSender<()>,
}

impl Drop for Shutdown {
    #[inline(always)]
    fn drop(&mut self) {
        let _ = self.sender.send(());
    }
}

///Clipboard master.
///
///Tracks changes of clipboard and invokes corresponding callbacks.
///
///# Platform notes:
///
///- On `windows` it creates dummy window that monitors each clipboard change message.
pub struct Master<H> {
    handler: H,
    sender: SyncSender<()>,
    recv: Receiver<()>
}

impl<H: ClipboardHandler> Master<H> {
    #[inline(always)]
    ///Creates new instance.
    pub fn new(handler: H) -> io::Result<Self> {
        let (sender, recv) = sync_channel(0);

        Ok(Self {
            handler,
            sender,
            recv,
        })
    }

    #[inline(always)]
    ///Creates shutdown channel.
    pub fn shutdown_channel(&self) -> Shutdown {
        Shutdown {
            sender: self.sender.clone()
        }
    }


    ///Starts Master by waiting for any change
    pub fn run(&mut self) -> io::Result<()> {
        let mut last_hash: u64 =  0;
        let mut ctx = match Clipboard::new() {
            Ok(context) => context,
            Err(err) => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to initialize clipboard: {:?}", err),
                ))
            }
        };

        loop {
            match self.recv.recv_timeout(self.handler.sleep_interval()) {
                Ok(()) => break,
                //timeout
                Err(mpsc::RecvTimeoutError::Timeout) => {},
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            let changed = match ctx.get_text() {
                Ok(contents) => {
                    let mut s = DefaultHasher::new();
                    contents.as_bytes().hash(&mut s);
                    let new_hash = s.finish();
                    let changed = new_hash != last_hash;
                    last_hash = new_hash;
                    changed
                }
                Err(_err) => {
                    last_hash = 0;
                    false
                },
            };
            if changed {
                match self.handler.on_clipboard_change() {
                    CallbackResult::Next => (),
                    CallbackResult::Stop => break,
                    CallbackResult::StopWithError(error) => {
                        return Err(error);
                    }
                }
            }

        }

        Ok(())
    }
}