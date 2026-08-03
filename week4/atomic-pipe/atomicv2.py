import os
from collections import deque


class Message:
    def __init__(self, id: int, last: int, payload: bytes):
        self.id = id
        self.last = last
        self.payload = payload
        self.BUF = 4096

    def convert(self) -> bytes:
        shift = 22
        if self.last:
            word = self.id | ((self.last & 1) << 22)
            return (
                word.to_bytes(3, "big")
                + len(self.payload).to_bytes(2, "big")
                + self.payload
            )
        else:
            word = self.id | ((self.last & 1) << 22)
            return word.to_bytes(3, "big") + self.payload


class Pipe:
    """
    A pipe holds multiple states including an actual os pipe (alongside file descriptors) and a couple of buffers
    """

    def __init__(self):
        (self.r, self.w) = os.pipe()
        self.tail_buf = []
        self.accumulator = {}
        self.ready_buf = deque()
        self.BUF_LIMIT = 4096  # TODO make this configurable from fnctl

    """
    Write contract is simple, submit data + identifier
    """

    def write(self, id: int, data: bytes):
        # step 1: distribute the byte stream into messages
        messages = chunk(id, data, self.BUF_LIMIT)
        # step 2: write the messages to the actual pipe
        for message in messages:
            try:
                # TODO: detect partial writes (from man page: All n bytes are written atomically; write(2) may block if
                # there is not room for n bytes to be written immediately)
                os.write(self.w, data)
            except Exception as e:
                print(f"Write failed due to :{e}")

    """
    Read is much more complicated, it needs to reassemble the interleaved messages and provide complete messages to the user
    """

    def read(self) -> (int, list):
        # step 1: read from the os.read()
        while len(self.ready_buf) == 0:
            buf = os.read(self.r, 4092)
            if len(buf) < 1:
                print(f"Buffer is invalid: {len(buf)}")
                continue
            self.tail_buf.extend(buf)
            # step 2: do I have a complete frame? if not, return with an empty message
            ptx = 0
            while ptx < len(self.tail_buf):
                # Case 1: we have an incomplete header ( < 3 bytes)
                # check for a 3 byte header
                first = self.tail_buf[ptx]
                last_bit = first >> 6 and 1
                if len(self.tail_buf[ptx:]) < 3:
                    # we don't have enough to read the header
                    break
                if not last_bit:
                    header = int.from_bytes(self.tail_buf[ptx : ptx + 3], "big")
                    id = header & ((1 << 22) - 1)
                    header_size = 3
                    remaining = len(self.tail_buf) - ptx
                    frame_len = header_size + 4092
                    if remaining < frame_len:
                        break
                    # if the tail buffer has 4092 bytes worth of data:
                    # only commit the pointer when a frame is confirmed
                    ptx += 3
                    # if not, we have a complete frame
                    if id in self.accumulator:
                        self.accumulator[id].extend(self.tail_buf[ptx : ptx + 4092])
                    else:
                        self.accumulator[id] = self.tail_buf[ptx : ptx + 4092]
                    ptx += 4092
                else:
                    if len(self.tail_buf[ptx:]) < 5:
                        break
                    # the header is 5 bytes
                    header = int.from_bytes(self.tail_buf[ptx : ptx + 3], "big")
                    id = header & ((1 << 22) - 1)
                    size = int.from_bytes(self.tail_buf[ptx : ptx + 2], "big")
                    header_size = 5
                    remaining = len(self.tail_buf) - ptx
                    frame_len = header_size + size
                    if size == 0:
                        print("nothing to read, breaking from loop")
                        break
                    if remaining < frame_len:
                        break
                    # only commit the pointer when a frame is confirmed
                    ptx += 5
                    if id in self.accumulator:
                        self.accumulator[id].extend(self.tail_buf[ptx : ptx + size])
                    else:
                        self.accumulator[id] = self.tail_buf[ptx : ptx + size]
                    ptx += size
                    # if I am writing a terminal/last frame, I can push the entire message to the ready output queue
                    self.ready_buf.append((id, self.accumulator[id]))
                    del self.accumulator[id]
            # if we've traversed a frame, or found out an incomplete one, let's flush the buffer
            del self.tail_buf[0:ptx]
        return self.ready_buf.popleft()

    def close_read(self):
        os.close(self.r)

    def close_write(self):
        os.close(self.w)


def chunk(pid: int, data: bytes, limit: int) -> list[bytes]:
    messages = []
    if len(data) > limit:
        chunk = [0] * 4092
        while len(data) > limit:
            chunk = data[:4092]
            message = Message(pid, 0, chunk).convert()
            print(f"chunk is writing {len(message)} worth of message into messagaes")
            messages.append(message)
            data = data[4092:]
    if len(data) != 0:
        # for the last payload, the header needs two more bytes
        m = Message(pid, 1, data).convert()
        messages.append(m)
    return messages
