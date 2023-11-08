# Generate the converted dictionary

all:
	cargo run
	arm-none-eabi-objcopy \
		-I binary \
		-O elf32-littlearm \
		--change-section-address .data=0x10200000 \
		lapwing-base.bin lapwing-base.elf
	elf2uf2-rs lapwing-base.elf lapwing-base.uf2
	copy lapwing-base.uf2 ~/david
		# -B armv6s-m \
