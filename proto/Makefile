# Make for silly stuff

all:
	echo all is not a useful target.

gdbserver:
	probe-rs gdb --chip RP2040 --protocol swd

gdb:
	arm-none-eabi-gdb -q -x debug.gdb target/thumbv6m-none-eabi/debug/proto

# Start the jlink server so gdb can program the board.
jlink:
	env -u DISPLAY \
	    JLinkGDBServer -strict -device RP2040_M0_0 -if SWD -vd

rtt:
	socat - TCP4:localhost:19021 | \
		defmt-print -e target/thumbv6m-none-eabi/debug/proto

semi:
	socat - TCP4:localhost:2333
