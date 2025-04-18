use crate::kbct::Result;
use mio::event::{Event, Source};
use mio::unix::SourceFd;
use mio::{Events, Interest, Poll, Token};
use std::collections::HashMap;

const EVENTS_CAPACITY: usize = 1024;

pub struct EventLoop {
	events: Events,
	registrar: EventLoopRegistrar,
}

pub struct EventLoopRegistrar {
	poll: Poll,
	running: bool,
	handlers: HashMap<Token, Box<dyn EventObserver>>,
	last_token: usize,
}

pub enum ObserverResult {
	Nothing,
	Unsubcribe,
	Terminate { status: i32 },
	SubscribeNew(Vec<Box<dyn EventObserver>>),
}

impl EventLoop {
	pub fn new() -> Result<EventLoop> {
		Ok(EventLoop {
			events: Events::with_capacity(EVENTS_CAPACITY),
			registrar: EventLoopRegistrar {
				poll: Poll::new()?,
				running: true,
				handlers: HashMap::new(),
				last_token: 0,
			},
		})
	}

	pub fn run(&mut self) -> Result<()> {
		while self.registrar.running {
			match self.registrar.poll.poll(&mut self.events, None) {
				Ok(_) => (),
				Err(e) => match e.kind() {
					std::io::ErrorKind::Interrupted => {
						println!("Ignoring error: {}", e);
						continue;
					}
					_ => return Err(crate::kbct::KbctError::IOError(e)),
				},
			}
			for ev in self.events.iter() {
				let handler = self.registrar.handlers.get_mut(&ev.token()).unwrap();
				match handler.on_event(ev)? {
					ObserverResult::Nothing => {}
					ObserverResult::Unsubcribe => {
						handler
							.get_source_fd()
							.deregister(self.registrar.poll.registry())?;
						self.registrar.handlers.remove(&ev.token());
					}
					ObserverResult::Terminate { status: _status } => {
						self.registrar.running = false;
					}
					ObserverResult::SubscribeNew(observers) => {
						for obs in observers {
							EventLoop::do_register_observer(&mut self.registrar, obs)?;
						}
					}
				}
			}
		}
		Ok(())
	}

	pub fn register_observer(&mut self, obs: Box<dyn EventObserver>) -> Result<()> {
		Ok(EventLoop::do_register_observer(&mut self.registrar, obs)?)
	}

	fn do_register_observer(
		reg: &mut EventLoopRegistrar,
		obs: Box<dyn EventObserver>,
	) -> Result<()> {
		let mut fd = obs.get_source_fd();
		let token = Token(reg.last_token);
		reg.last_token += 1;
		reg.poll
			.registry()
			.register(&mut fd, token, Interest::READABLE)?;
		assert!(
			reg.handlers.get(&token).is_none(),
			"Token handler is already set"
		);
		reg.handlers.insert(token, obs);
		Ok(())
	}
}

pub trait EventObserver {
	fn on_event(&mut self, _: &Event) -> Result<ObserverResult>;
	fn get_source_fd(&self) -> SourceFd;
}
