import serial

s = serial.Serial("/dev/ttyACM0")

with open("day01-input.txt") as f:
    for line in f.readlines():
        s.write(line.encode("ascii"))

    s.write(b'\n')
    s.write(1)  # no effin' clue why this works but 0 not

print(f"Part 1: {int.from_bytes(s.read(4), byteorder='little')}")
print(f"Part 2: {int.from_bytes(s.read(4), byteorder='little')}")
