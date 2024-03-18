# Generate the converted dictionary

all:
	cargo run
	arm-none-eabi-objcopy \
		-I binary \
		-O elf32-littlearm \
		--change-section-address .data=0x10100000 \
		phoenix.bin phoenix.elf

# elf2uf2-rs phoenix.elf phoenix.uf2
# copy phoenix.uf2 ~/david

#		-B armv6s-m \
