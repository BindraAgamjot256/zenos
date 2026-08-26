//! QEMU Machine Protocol (QMP) coordination for controlled guest pauses.
//!
//! The guest writes binary markers to its serial output. This module removes those
//! markers from the user-visible output, pauses or resumes QEMU through QMP, and
//! acknowledges each marker over the guest's serial input. Keeping this handshake
//! outside the guest lets us compare elapsed wall time with the time observed by
//! the guest while accounting for the interval in which the VM was stopped.

use std::{
    io::{self, BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    process::{Command, Stdio},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use crate::{ensure_success, Result};

const QMP_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const QMP_PAUSE_DURATION: Duration = Duration::from_millis(10);

// These byte sequences form a small control protocol embedded in the otherwise
// human-readable serial stream. They deliberately use bytes unlikely to occur in
// UTF-8 output, but `forward_serial` still treats partial matches carefully.
const SERIAL_PAUSE_MARKER: &[u8] = &[0xFF, 0xFF, 0x00, 0x00];
const SERIAL_COMPLETE_MARKER: &[u8] = &[0xFF, 0xFF, 0x00, 0x01];
const SERIAL_PAUSE_ACK: u8 = 0xAC;
const SERIAL_COMPLETE_ACK: u8 = 0xAD;

#[derive(Debug, Clone, Copy)]
enum SerialEvent {
    PauseRequested(Instant),
    PauseCompleted(Instant),
}

pub(crate) fn run(command: &mut Command) -> Result<()> {
    // Ask the OS for an unused port before constructing QEMU's command line.
    // QEMU itself becomes the listener when it starts.
    let qmp_addr = reserve_qmp_address()?;

    command.args([
        "-qmp",
        &format!("tcp:{qmp_addr},server=on,wait=off,nodelay=on"),
    ]);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());

    println!("[RUN] Command: {command:#?}");
    io::stdout().flush()?;

    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to launch QEMU: {error}"))?;
    let serial_input = child
        .stdin
        .take()
        .ok_or("failed to capture QEMU serial input")?;
    let serial = child
        .stdout
        .take()
        .ok_or("failed to capture QEMU serial output")?;

    // Serial forwarding remains on this thread while all blocking QMP traffic is
    // isolated in a worker. The readiness channel prevents serial processing from
    // beginning until QMP negotiation has succeeded.
    let (marker_tx, marker_rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);

    // Serial bytes and QMP diagnostics share stdout. Serialize complete writes so
    // a timing line cannot be inserted in the middle of guest output.
    let output_lock = Arc::new(Mutex::new(()));
    let qmp_output_lock = Arc::clone(&output_lock);
    let qmp_thread = thread::spawn(move || {
        let result = run_qmp_worker(
            qmp_addr,
            marker_rx,
            ready_tx,
            serial_input,
            &qmp_output_lock,
        );
        if let Err(error) = &result {
            let _ = write_output_line(&qmp_output_lock, &format!("[QMP] {error}"));
        }
        result
    });

    match ready_rx.recv() {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = qmp_thread.join();
            return Err(error.into());
        }
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = qmp_thread.join();
            return Err(format!("QMP worker exited during startup: {error}").into());
        }
    }

    let serial_result = forward_serial(serial, &marker_tx, &output_lock);

    // Closing the last sender is the worker's normal shutdown signal after the
    // serial stream reaches EOF.
    drop(marker_tx);

    if serial_result.is_err() {
        let _ = child.kill();
    }

    let status = child.wait()?;
    let qmp_result = qmp_thread
        .join()
        .map_err(|_| "QMP worker thread panicked")?;

    serial_result?;
    qmp_result.map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
    ensure_success(status, "QEMU")
}

fn reserve_qmp_address() -> io::Result<SocketAddr> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    listener.local_addr()
}

fn run_qmp_worker(
    address: SocketAddr,
    marker_rx: mpsc::Receiver<SerialEvent>,
    ready_tx: mpsc::SyncSender<std::result::Result<(), String>>,
    mut serial_input: impl Write,
    output_lock: &Mutex<()>,
) -> std::result::Result<(), String> {
    let mut qmp = match QmpConnection::connect(address) {
        Ok(qmp) => qmp,
        Err(error) => {
            let _ = ready_tx.send(Err(error.clone()));
            return Err(error);
        }
    };

    println!("[QMP] Connected to {address}");
    ready_tx
        .send(Ok(()))
        .map_err(|error| format!("failed to report QMP readiness: {error}"))?;

    let mut pause_sequence = 0u64;
    // A completed marker is paired with the most recent pause request so the
    // guest-observed interval can be corrected by the actual QMP stop interval.
    let mut measurement_started = None;

    loop {
        match marker_rx.recv() {
            Ok(SerialEvent::PauseRequested(started_at)) => {
                pause_sequence = pause_sequence.wrapping_add(1);
                let timing = qmp.pause_for(QMP_PAUSE_DURATION, pause_sequence, || {
                    serial_input
                        .write_all(&[SERIAL_PAUSE_ACK])
                        .and_then(|()| serial_input.flush())
                        .map_err(|error| {
                            format!("failed to acknowledge serial pause marker: {error}")
                        })
                })?;

                write_output_line(
                    output_lock,
                    &format!(
                        "[QMP] pause #{}: controlled {:?}, VM stopped {:?}",
                        pause_sequence, timing.controlled, timing.vm_stopped
                    ),
                )
                .map_err(|error| format!("failed to write QMP timing: {error}"))?;

                measurement_started = Some((pause_sequence, started_at, timing.vm_stopped));
            }
            Ok(SerialEvent::PauseCompleted(completed_at)) => {
                let Some((sequence, started_at, vm_stopped)) = measurement_started.take() else {
                    return Err("received a pause completion without a start marker".into());
                };
                let wall_time = completed_at.duration_since(started_at);
                // A well-behaved guest clock should not advance while the VM is
                // stopped. Saturation also makes this robust to tiny timestamp
                // ordering differences at the host-side measurement boundaries.
                let expected_guest_time = wall_time.saturating_sub(vm_stopped);

                write_output_line(
                    output_lock,
                    &format!(
                        "[QMP] measurement #{sequence}: wall {wall_time:?}, expected guest clock \
                         {expected_guest_time:?}"
                    ),
                )
                .map_err(|error| format!("failed to write QMP measurement: {error}"))?;

                serial_input
                    .write_all(&[SERIAL_COMPLETE_ACK])
                    .and_then(|()| serial_input.flush())
                    .map_err(|error| {
                        format!("failed to acknowledge serial completion marker: {error}")
                    })?;
            }
            Err(_) => return Ok(()),
        }
    }
}

struct PauseTiming {
    controlled: Duration,
    vm_stopped: Duration,
}

struct QmpConnection {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
}

impl QmpConnection {
    fn connect(address: SocketAddr) -> std::result::Result<Self, String> {
        let deadline = Instant::now() + QMP_CONNECT_TIMEOUT;
        let stream = loop {
            match TcpStream::connect(address) {
                Ok(stream) => break stream,
                Err(error) if Instant::now() < deadline => {
                    let _ = error;
                    thread::sleep(Duration::from_millis(1));
                }
                Err(error) => {
                    return Err(format!("failed to connect to QMP at {address}: {error}"));
                }
            }
        };

        stream
            .set_nodelay(true)
            .map_err(|error| format!("failed to configure QMP socket: {error}"))?;
        stream
            .set_read_timeout(Some(QMP_CONNECT_TIMEOUT))
            .map_err(|error| format!("failed to configure QMP timeout: {error}"))?;

        let writer = stream
            .try_clone()
            .map_err(|error| format!("failed to clone QMP socket: {error}"))?;
        let mut qmp = Self {
            reader: BufReader::new(stream),
            writer,
        };

        // QMP sends a greeting before accepting commands; capability negotiation
        // must be the first command in every new session.
        let greeting = qmp.read_message()?;
        if !greeting.contains("\"QMP\"") {
            return Err(format!("invalid QMP greeting: {greeting}"));
        }

        qmp.send_command("qmp_capabilities", "zenos-capabilities")?;
        qmp.wait_for_response()?;
        Ok(qmp)
    }

    fn pause_for(
        &mut self,
        duration: Duration,
        sequence: u64,
        before_resume: impl FnOnce() -> std::result::Result<(), String>,
    ) -> std::result::Result<PauseTiming, String> {
        let stop_id = format!("zenos-stop-{sequence}");
        let cont_id = format!("zenos-cont-{sequence}");

        self.send_command("stop", &stop_id)?;
        // The command response and asynchronous STOP event may arrive in either
        // order, so do not begin the controlled delay until both are observed.
        let stop_event_at = self.wait_for_response_and_event("STOP")?;
        let stopped_at = Instant::now();

        wait_until(stopped_at + duration);
        // Acknowledge only after the pause has elapsed but before resuming. This
        // lets the guest block on serial input without consuming guest CPU time.
        before_resume()?;
        let cont_sent = self.send_command("cont", &cont_id)?;
        let resume_event_at = self.wait_for_response_and_event("RESUME")?;

        Ok(PauseTiming {
            // Includes the requested delay, acknowledgement write, and the time
            // needed to submit `cont`.
            controlled: cont_sent.duration_since(stopped_at),
            // Uses QEMU's asynchronous events as the authoritative boundaries of
            // the interval during which the VM was actually stopped.
            vm_stopped: resume_event_at.duration_since(stop_event_at),
        })
    }

    fn send_command(&mut self, command: &str, id: &str) -> std::result::Result<Instant, String> {
        let message = format!("{{\"execute\":\"{command}\",\"id\":\"{id}\"}}\r\n");
        self.writer
            .write_all(message.as_bytes())
            .and_then(|()| self.writer.flush())
            .map_err(|error| format!("failed to send QMP {command}: {error}"))?;
        Ok(Instant::now())
    }

    fn wait_for_response(&mut self) -> std::result::Result<(), String> {
        loop {
            let message = self.read_message()?;
            if message.contains("\"return\"") {
                return Ok(());
            }
            if message.contains("\"error\"") {
                return Err(format!("QMP command failed: {message}"));
            }
        }
    }

    fn wait_for_response_and_event(
        &mut self,
        expected_event: &str,
    ) -> std::result::Result<Instant, String> {
        let mut received_response = false;
        let mut event_at = None;

        // QMP events are asynchronous and are not guaranteed to be adjacent to,
        // or ordered after, the matching command response.
        while !received_response || event_at.is_none() {
            let message = self.read_message()?;
            if message.contains("\"return\"") {
                received_response = true;
            } else if message.contains("\"error\"") {
                return Err(format!("QMP command failed: {message}"));
            }

            if message.contains("\"event\"") && message.contains(&format!("\"{expected_event}\"")) {
                event_at = Some(Instant::now());
            }
        }

        event_at.ok_or_else(|| format!("QMP did not emit {expected_event}"))
    }

    fn read_message(&mut self) -> std::result::Result<String, String> {
        let mut message = String::new();
        let bytes = self
            .reader
            .read_line(&mut message)
            .map_err(|error| format!("failed to read QMP response: {error}"))?;
        if bytes == 0 {
            return Err("QMP connection closed unexpectedly".into());
        }
        Ok(message)
    }
}

fn wait_until(deadline: Instant) {
    // Sleeping handles most of the delay efficiently; spinning only for the final
    // millisecond avoids scheduler granularity dominating a short 10 ms pause.
    const SPIN_THRESHOLD: Duration = Duration::from_micros(1000);

    loop {
        let now = Instant::now();
        if now >= deadline {
            return;
        }

        let remaining = deadline - now;
        if remaining > SPIN_THRESHOLD {
            thread::sleep(remaining - SPIN_THRESHOLD);
        } else {
            std::hint::spin_loop();
        }
    }
}

fn forward_serial(
    mut serial: impl Read,
    marker_tx: &mpsc::Sender<SerialEvent>,
    output_lock: &Mutex<()>,
) -> io::Result<()> {
    let mut input = [0; 1024];
    // `read` can split a marker at any byte. Hold a possible marker prefix until
    // enough bytes arrive to either recognize it or release it as normal output.
    let mut candidate = Vec::with_capacity(SERIAL_PAUSE_MARKER.len());
    // Buffer by line so QMP status messages can be emitted atomically between
    // guest lines rather than interleaving with arbitrary serial chunks.
    let mut output_line = Vec::new();

    loop {
        let bytes = serial.read(&mut input)?;
        if bytes == 0 {
            output_line.extend_from_slice(&candidate);
            if !output_line.is_empty() {
                write_output(output_lock, &output_line)?;
            }
            return Ok(());
        }

        for &byte in &input[..bytes] {
            candidate.push(byte);

            loop {
                let is_control_prefix = SERIAL_PAUSE_MARKER.starts_with(&candidate)
                    || SERIAL_COMPLETE_MARKER.starts_with(&candidate);
                if is_control_prefix {
                    let event = if candidate == SERIAL_PAUSE_MARKER {
                        Some(SerialEvent::PauseRequested(Instant::now()))
                    } else if candidate == SERIAL_COMPLETE_MARKER {
                        Some(SerialEvent::PauseCompleted(Instant::now()))
                    } else {
                        None
                    };

                    if let Some(event) = event {
                        marker_tx.send(event).map_err(|error| {
                            io::Error::new(
                                io::ErrorKind::BrokenPipe,
                                format!("QMP worker is unavailable: {error}"),
                            )
                        })?;
                        candidate.clear();
                    }
                    break;
                }

                // The candidate is not a control prefix. Release its first byte,
                // then retry the remainder because it may itself begin a marker.
                let byte = candidate.remove(0);
                output_line.push(byte);
                if byte == b'\n' {
                    write_output(output_lock, &output_line)?;
                    output_line.clear();
                }
                if candidate.is_empty() {
                    break;
                }
            }
        }
    }
}

fn write_output(output_lock: &Mutex<()>, bytes: &[u8]) -> io::Result<()> {
    let _guard = output_lock
        .lock()
        .map_err(|_| io::Error::other("output lock is poisoned"))?;
    let mut output = io::stdout().lock();
    output.write_all(bytes)?;
    output.flush()
}

fn write_output_line(output_lock: &Mutex<()>, line: &str) -> io::Result<()> {
    let _guard = output_lock
        .lock()
        .map_err(|_| io::Error::other("output lock is poisoned"))?;
    let mut output = io::stdout().lock();
    writeln!(output, "{line}")?;
    output.flush()
}
