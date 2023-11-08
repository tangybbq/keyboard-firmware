#! /usr/bin/env python3

import struct

def create_uf2(binary_filename, uf2_filename, start_address):
    # Constants for UF2 block header
    UF2_BLOCK_SIZE = 512
    UF2_HEADER_SIZE = 32
    UF2_PAYLOAD_SIZE = 256  # Set payload size to 256 bytes
    UF2_MAGIC_START0 = 0x0A324655  # "UF2\n"
    UF2_MAGIC_START1 = 0x9E5D5157  # Randomly selected
    UF2_MAGIC_END = 0x0AB16F30    # Another random number

    with open(binary_filename, 'rb') as bin_file:
        binary_data = bin_file.read()

    # Calculate the number of blocks and pad the binary data if necessary
    num_blocks = (len(binary_data) + UF2_PAYLOAD_SIZE - 1) // UF2_PAYLOAD_SIZE
    padded_binary_data = binary_data.ljust(num_blocks * UF2_PAYLOAD_SIZE, b'\xFF')
    padding = b'\x00' * (UF2_BLOCK_SIZE - UF2_HEADER_SIZE - UF2_PAYLOAD_SIZE - 4)
    ending_magic = struct.pack('<I', UF2_MAGIC_END)

    with open(uf2_filename, 'wb') as uf2_file:
        for block_num in range(num_blocks):
            block_data = padded_binary_data[block_num*UF2_PAYLOAD_SIZE:(block_num+1)*UF2_PAYLOAD_SIZE]
            payload_size = len(block_data)
            block_header = struct.pack('<IIIIIIII',
                                       UF2_MAGIC_START0,
                                       UF2_MAGIC_START1,
                                       0x200,
                                       start_address + block_num * UF2_PAYLOAD_SIZE,
                                       payload_size,
                                       block_num,
                                       num_blocks,
                                       0xe48bff56,
            )
            # Write the header and the payload to the UF2 file
            uf2_file.write(block_header)
            uf2_file.write(block_data)
            uf2_file.write(padding)
            uf2_file.write(ending_magic)

# Replace 'your_binary.bin', 'your_output.uf2', and 0xADDRESS with your values
create_uf2('lapwing-base.bin', 'lapwing-base.uf2', 0x10200000)
