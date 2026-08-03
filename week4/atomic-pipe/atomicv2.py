import os
from collections import deque


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

    def read(self) -> list:
        # step 1: read from the os.read()
        buf = os.read(self.r, 4092)
        if len(buf) < 1:
            print(f"Buffer is invalid: {len(buf)}")
        self.tail_buf.extend(buf)
        # step 2: do I have a complete frame? if not, return with an empty message
        ptx = 0
        while ptx < len(self.tail_buf):
            # Case 1: we have an incomplete header ( < 3 bytes)
            # check for a 3 byte header
            first = self.tail_buf[ptx]
            last_bit = first >> 6 and 1
            if len(self.tail_buf) < 3:
                # we don't have enough to read the header
                break
            if not last_bit:
                header = int.from_bytes(self.tail_buf[ptx : ptx + 3], "big")
                ptx += 3
                id = header & ((1 << 22) - 1)
                # if the tail buffer has 4092 bytes worth of data:
                if len(self.tail_buf[ptx:]) < 4092:
                    break
                # if not, we have a complete frame
                if id in self.accumulator:
                    self.accumulator[id].append(self.tail_buf[ptx : ptx + 4092])
                else:
                    self.accumulator[id] = self.tail_buf[ptx : ptx + 4092]
                ptx += 4092
            else:
                if len(self.tail_buf) < 5:
                    break
                # the header is 5 bytes
                header = int.from_bytes(self.tail_buf[ptx : ptx + 3], "big")
                ptx += 3
                id = header & ((1 << 22) - 1)
                size = int.from_bytes(self.tail_buf[ptx : ptx + 2], "big")
                ptx += 2
                if size == 0:
                    print("nothing to read, breaking from loop")
                    break
                if len(self.tail_buf[ptx : ptx + size]) < size:
                    break
                if id in self.accumulator:
                    self.accumulator[id].append(self.tail_buf[ptx : ptx + size])
                else:
                    self.accumulator[id] = self.tail_buf[ptx : ptx + size]
                ptx += size

        return []


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
