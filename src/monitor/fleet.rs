use super::Session;
use crate::{
    fleet::{Availability, DeviceState, Fleet},
    model::DeviceIdentity,
    transport::{BatteryTransport, DeviceDiscovery},
};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime},
};

struct Worker {
    generation: u64,
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl Worker {
    fn cancel(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub fn watch_all<T>(
    factory: impl Fn() -> T + Send + Sync + 'static,
    selector: Option<&str>,
    stop: &AtomicBool,
    mut publish: impl FnMut(Fleet),
) where
    T: BatteryTransport + DeviceDiscovery,
{
    let factory = Arc::new(factory);
    let mut discovery = factory();
    let mut workers: HashMap<String, Worker> = HashMap::new();
    let (sender, receiver) = mpsc::channel();
    let mut fleet = Fleet::default();
    let mut previous = None;
    let mut next_scan = Instant::now();
    let mut generation = 0_u64;
    while !stop.load(Ordering::Relaxed) {
        while let Ok((event_generation, update)) = receiver.try_recv() {
            let update: DeviceState = update;
            if workers.get(&update.identity.id).is_some_and(|worker| {
                worker.generation == event_generation && !worker.stop.load(Ordering::Relaxed)
            }) {
                fleet.apply(update);
            }
        }
        if Instant::now() >= next_scan {
            match discovery.discover() {
                Ok(mut identities) => {
                    identities.retain(|identity| {
                        selector.is_none_or(|selected| {
                            selected == identity.driver || selected == identity.id
                        })
                    });
                    fleet.discovery_error = None;
                    fleet.reconcile(&identities);
                    for (id, worker) in &workers {
                        if !identities.iter().any(|identity| &identity.id == id) {
                            worker.cancel();
                        }
                    }
                    let finished: Vec<_> = workers
                        .iter()
                        .filter(|(_, worker)| worker.handle.is_finished())
                        .map(|(id, _)| id.clone())
                        .collect();
                    for id in finished {
                        if let Some(worker) = workers.remove(&id)
                            && worker.handle.join().is_err()
                        {
                            crate::diagnostics::log(&format!("Device worker panicked: {id}"));
                            if let Some(state) = fleet.devices.get_mut(&id) {
                                state.availability = Availability::Offline;
                                state.error = Some("设备监控线程异常".into());
                            }
                        }
                    }
                    for identity in identities {
                        if workers.contains_key(&identity.id) {
                            continue;
                        }
                        let factory = Arc::clone(&factory);
                        let sender = sender.clone();
                        let local_stop = Arc::new(AtomicBool::new(false));
                        let worker_stop = Arc::clone(&local_stop);
                        let id = identity.id.clone();
                        generation += 1;
                        let event_generation = generation;
                        let handle = thread::spawn(move || {
                            read_device(factory(), &identity, &worker_stop, |state| {
                                let _ = sender.send((event_generation, state));
                            });
                        });
                        workers.insert(
                            id,
                            Worker {
                                generation,
                                stop: local_stop,
                                handle,
                            },
                        );
                    }
                }
                Err(error) => {
                    let error = format!("设备扫描失败：{error:#}");
                    if fleet.discovery_error.as_ref() != Some(&error) {
                        crate::diagnostics::log(&error);
                    }
                    fleet.discovery_error = Some(error);
                }
            }
            next_scan = Instant::now() + Duration::from_secs(3);
        }
        if previous.as_ref() != Some(&fleet) {
            publish(fleet.clone());
            previous = Some(fleet.clone());
        }
        thread::sleep(Duration::from_millis(100));
    }
    for worker in workers.values() {
        worker.cancel();
    }
    for (_, worker) in workers {
        let _ = worker.handle.join();
    }
}

fn read_device(
    mut transport: impl BatteryTransport,
    identity: &DeviceIdentity,
    stop: &AtomicBool,
    mut publish: impl FnMut(DeviceState),
) {
    let mut state = DeviceState::new(identity.clone());
    publish(state.clone());
    let result = (|| {
        let mut connection = transport.connect(Some(&identity.id))?;
        let mut session = Session::new(
            identity.clone(),
            Instant::now(),
            None,
            connection.report_kind(),
        );
        while !stop.load(Ordering::Relaxed) {
            let battery = connection.read()?;
            if let Some(status) = session.observe(battery, Instant::now(), SystemTime::now()) {
                state.availability = if matches!(status, crate::model::Status::Live(_, _)) {
                    Availability::Live
                } else {
                    Availability::Stale
                };
                state.reading = status.reading();
                publish(state.clone());
            }
        }
        anyhow::Ok(())
    })();
    if let Err(error) = result {
        let error = format!("{error:#}");
        crate::diagnostics::log(&format!("{}: {error}", identity.id));
        state.availability = Availability::Offline;
        state.error = Some(error);
        publish(state);
    }
}
