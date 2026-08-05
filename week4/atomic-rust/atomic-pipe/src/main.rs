use atomic_pipe::pipe::Pipe;
use std::io::{Read, Write};

const N: usize = 400_000;
const WRITERS: usize = 4;

fn reap() {
    let mut status = 0;
    for _ in 0..WRITERS {
        unsafe { libc::waitpid(-1, &mut status, 0) };
    }
}

// Control: no framing. Each child does one write_all of N bytes, which the
// kernel splits as the pipe fills. Runs > WRITERS means a write was torn.
fn raw_pipe_demo() {
    let (mut r, w) = std::io::pipe().unwrap();
    let mut w = Some(w);
    for i in 0..WRITERS {
        if unsafe { libc::fork() } == 0 {
            let buf = vec![65u8 + i as u8; N];
            w.as_mut().unwrap().write_all(&buf).unwrap();
            unsafe { libc::_exit(0) }
        }
    }
    drop(w.take());

    let mut all = Vec::new();
    r.read_to_end(&mut all).unwrap();
    reap();

    let runs = all.windows(2).filter(|p| p[0] != p[1]).count() + 1;
    println!("=== raw pipe, no framing ===");
    println!("{} bytes read, {} runs", all.len(), runs);
    println!(
        "{}\n",
        if runs > WRITERS {
            format!("TORN: {} writes were interleaved mid-payload", runs - WRITERS)
        } else {
            "intact (no interleaving happened this run)".to_string()
        }
    );
}

fn framed_pipe_demo() {
    let mut p = Pipe::new().unwrap();
    for i in 0..WRITERS {
        if unsafe { libc::fork() } == 0 {
            let buf = vec![65u8 + i as u8; N];
            let pid = unsafe { libc::getpid() } as u32;
            p.write(pid, &buf);
            p.close_write();
            unsafe { libc::_exit(0) }
        }
    }
    p.close_write();

    println!("=== atomic pipe, framed ===");
    let mut ids: Vec<u32> = Vec::new();
    while let Some((id, msg)) = p.read() {
        let intact = msg.iter().all(|&b| b == msg[0]);
        println!(
            "id {:>7}  len {:>6}  byte {:?}  {}",
            id,
            msg.len(),
            msg[0] as char,
            if intact { "intact" } else { "TORN" }
        );
        assert!(intact, "writer {} was torn", id);
        assert_eq!(N, msg.len());
        ids.push(id);
    }
    reap();

    let mut distinct = ids.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(WRITERS, ids.len(), "expected {} messages, got {:?}", WRITERS, ids);
    assert_eq!(WRITERS, distinct.len(), "duplicate ids in {:?}", ids);

    // A writer interleaved iff its frames are not contiguous in arrival order.
    let split: Vec<u32> = distinct
        .iter()
        .copied()
        .filter(|&id| {
            let first = p.frame_order.iter().position(|&x| x == id).unwrap();
            let last = p.frame_order.iter().rposition(|&x| x == id).unwrap();
            let count = p.frame_order.iter().filter(|&&x| x == id).count();
            last - first + 1 != count
        })
        .collect();
    println!("{} frames parsed", p.frame_order.len());
    if split.is_empty() {
        println!("no interleaving observed - raise N");
    } else {
        println!("writers split by others mid-stream: {:?}", split);
    }
}

fn main() {
    raw_pipe_demo();
    framed_pipe_demo();
}
