#! /bin/bash

probe-rs download --chip RP2040 --protocol swd --format bin --base-address 0x10200000 main.bindict
