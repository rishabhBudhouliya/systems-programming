# Goal: Create a pipe that guarantees atomic writes post PIPE_BUF threshold

## How does an anonymous pipe (POSIX Pipe) work?

(r, w) = os.pipe()
r gives us a file descriptor that can be used to read from the pipe
w gives us a file descriptor that can be used to write to the pipe

Blocking behavior
If the pipe is full, write will be blocked until the pipe is drained

If all readers to the pipe are closed, the os will crash the process with a signal

Non blocking behavior
Even if the pipe is full, write is not blocked but partial writes are returned and 
it is the user's responsibility to manage completing those writes - the non atomic behavior
is managed by the user and in blocking behavior, the kernel manages that for us

## What is the problem we're solving?

|                  | `n ≤ PIPE_BUF`                                | `n > PIPE_BUF`                                      |
| ---------------- | --------------------------------------------- | --------------------------------------------------- |
| **blocking**     | all-or-nothing, blocks until room for all `n` | returns `n`, but may interleave with other writers  |
| **non-blocking** | writes all `n`, or `EAGAIN` — never partial   | partial write of whatever fits, or `EAGAIN` if full |


POSIX Pipe
We wish to implement a version of a pipe that sends atomic messages from multiple writers even when n > PIPE_BUF

## How will we achieve that?
Given the POSIX specification for it tells us that atomicity isn't guaranteed, we need to think of a solution for it
on userland

### The idea of atomic pipes

Step 1: Take the n sized input
Step 2: Divide it in `n/PIPE_BUF` chunks
Step 3: write to the pipe
Step 4: re-assemble the chunks since interleaving 

Thinking about multiple processess
P1 has 5000 bytes worth of data
P2 has 6000 bytes worth of data

same pipe, r, w = os.pipe()

P1 breaks into 5000 - 4096 = 904 bytes extra
P1's packet 1: 5000 - 904 = 4096
P2's packet 2: 904

P2 breaks into 6000 - 4096 = 1904 bytes extra
P2's packet 1: 4096
P2's packet 2: 1094

os.write(w, data)

Now, I need some sort of id for each packet because I don't know which process's packet I might be reading as a parent

q1) How would the parent know which packet is being sent? - use child's pid
q2) How would the parent know when to stop? - a message counter
as a writer, you would know the number of messages per message sent by a process
So let's say 2 chunks, counter = 2
counter-=1 to 

os.read()

[]
22 bits for pid
1 bit for end of message stream
big-endian scheme
[8][8][8]
