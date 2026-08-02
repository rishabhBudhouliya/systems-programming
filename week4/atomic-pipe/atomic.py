import os
import time
from collections import Counter, defaultdict
from itertools import groupby


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
    def __init__(self):
        (self.r, self.w) = os.pipe()
        self.PIPE_BUF = 4096
        self.PAYLOAD_SIZE_LIMIT = 4092

    def write(self, pid: int, buf: bytes):
        messages = chunk(pid, buf, self.PIPE_BUF)
        for message in messages:
            try:
                print(f"writing message with length: {len(message)}")
                os.write(self.w, message)
            except Exception as e:
                print(f"write failed due to: {e}")

    # read with offset
    def read(self, seen: dict, chunk_size: int, pids):
        buf = os.read(self.r, 4095)
        print(f"buffer being read is: {buf}")
        if buf == b"":
            raise ValueError("End of buffer")
        # buf is bytes, now check for header and payload
        itr_buffer = bytearray(buf)
        pointer = 0
        # iterate till 4092
        count = 1
        while pointer < len(itr_buffer):
            print(f"how many times have we run: {count}")
            first = itr_buffer[pointer]
            last_bit = first >> 6 and 1
            if not last_bit:
                print("entering the non last bit zone")
                header = int.from_bytes(itr_buffer[pointer : pointer + 3], "big")
                print(
                    f"here's the header: {itr_buffer[pointer : pointer + 3].hex(' ')}"
                )
                pointer += 3
                id = header & ((1 << 22) - 1)
                print(
                    f"the header is: {header} the process id: {id} and pids are: {pids}"
                )
                assert id in pids
                if id in seen:
                    seen[id].append(
                        itr_buffer[pointer : pointer + self.PAYLOAD_SIZE_LIMIT]
                    )
                else:
                    seen[id] = [itr_buffer[pointer : pointer + self.PAYLOAD_SIZE_LIMIT]]
                pointer += pointer + self.PAYLOAD_SIZE_LIMIT
            else:
                print("entering the last bit zone")
                header = int.from_bytes(itr_buffer[pointer : pointer + 3], "big")
                id = header & ((1 << 22) - 1)
                print(f"the header is: {header} process id: {id} and pids are: {pids}")
                pointer += 3
                size = int.from_bytes(itr_buffer[pointer : pointer + 2], "big")
                if size == 0:
                    print("nothing to read, breaking from loop")
                    break
                assert id in pids
                pointer += 2
                if id in seen:
                    seen[id].append(itr_buffer[pointer : pointer + size])
                else:
                    seen[id] = [itr_buffer[pointer : pointer + size]]
                pointer += size
                print(f"pointer is at: {pointer} vs buffer is: {len(itr_buffer)}")
            count += 1
            assert pointer == len(itr_buffer)
        return seen

    def close_read(self):
        try:
            os.close(self.r)
        except Exception as e:
            print(f"Unable to close pipe: {e}")

    def close_write(self):
        try:
            os.close(self.w)
        except Exception as e:
            print(f"Unable to close pipe: {e}")

    def close(self):
        try:
            os.close(self.w)
            os.close(self.r)
        except Exception as e:
            print(f"Unable to close the pipe: {e}")


# chunk per process
# 3 bytes for header  + 1 byte padding + 4092 bytes for payload
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


def pipe_concurrent_writes(N: int):
    # common pipe meant to be used by all
    # (r, w) = os.pipe()
    p = Pipe()
    # os.set_blocking(w, True)
    # attempt to create 4 processess
    pids = []
    for i in range(4):
        pid = os.fork()
        counter = i
        pids.append(pid)
        if pid == 0:
            # the idea is that once a child is spawned, it should not follow the loop and break out the loop
            break

    if pid == 0:
        try:
            payload = bytes([65 + counter]) * N
            current_pid = os.getpid()
            print(f"payload is {payload} with child pid: {current_pid}")
            p.write(current_pid, payload)
            # n = os.write(p.w, payload)
            # if n < len(payload):
            #     print(
            #         f"writer {counter}: {'TORN' if n < len(payload) else 'intact'} ({n}/{N})"
            #     )
        except BlockingIOError:
            print(f"writer {counter}: EAGAIN - nothing written")
        except Exception as e:
            print(f"couldn't write due to: {e}")
        finally:
            p.close_write()
            # os.close(p.w)
            os._exit(0)
    else:
        p.close_write()
        # os.close(p.w)
        buf = b""
        seen = {}
        # let the parent sleep for half a second
        while True:
            try:
                seen = p.read(seen, 4096, pids)
            except ValueError as e:
                break
            finally:
                print("sleeping until the entire buffer is not read")
                time.sleep(5)
            # chunk = os.read(p.r, 4096)
            # if chunk == b"":
            #     break
            # buf += chunk
        print(f"seen is {seen}")
        # print(f"read buffer is: {len(seen)}")
        print(f"writes are consistent : {checker(buf, N)}")
        for pid in pids:
            (_, status) = os.waitpid(pid, 0)
            print(
                f"Wait pid for {pid} with exit code: {os.waitstatus_to_exitcode(status)}"
            )


def checker(buf: bytes, N: int) -> bool:
    output = buf.decode()
    runs = [(k, len(list(g))) for k, g in groupby(output)]
    totals = Counter()
    for k, n in runs:
        totals[k] += n
    print(totals)
    if len(runs) > 4:
        print("".join(k for k, _ in runs))
        print([(k, n // 4096) for k, n in runs])
        print("Write might be torn!!")
    return len(runs) == 4


def main():
    N = 10000
    payload = bytes([65]) * N
    # result = chunk(10235, payload, 4096)
    # # i know that each ch
    # count = 0
    # for ch in result:
    #     itr_chunk = bytearray(ch)
    #     first = itr_chunk[0]
    #     last_bit = (first >> 6 and 1)
    #     if last_bit:
    #         header = int.from_bytes(itr_chunk[0:3], "big")
    #         size_payload = int.from_bytes(itr_chunk[3:5], "big")
    #         payload = itr_chunk[5:]
    #         count+=size_payload
    #     else:
    #         header = int.from_bytes(itr_chunk[0:3], "big")
    #         payload = itr_chunk[3:]
    #         count+=4092
    #         assert len(payload) == 4092
    # assert count == (4092 + 4092 + 1092)

    ###### Testing the atomic pipe
    # p = Pipe()
    # pids = [10112]
    # for pid in pids:
    #     p.write(pid, payload)

    # answer = {}
    # answer = p.read(answer, 4096)
    # p.read(answer, 4096)
    # p.read(answer, 4096)
    # print(f"here the final updated answer: {answer}")
    pipe_concurrent_writes(1)
    # p.close()
    # atomic_pipe()


if __name__ == "__main__":
    main()
