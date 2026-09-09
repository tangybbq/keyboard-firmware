# Porting ZMK

This document describes a plan to port ZMK to the mesa keyboards.  The following goals apply:
- Understand how ZMK is intended to be used and developed. I am very familiar with zephyr development, but much of what I've read about using ZMK seems to suggest a tuned build environment for it (as well as cloud builds). I want to understand how to set up a local build environment to be able to develop my support locally, as well as to understand how I can share both my board support (modules?) as well as any specific features I create, such as Dosh support.
- Initial focus will be on supporting only Dosh on the mesa line. Development will probably start on the mesa1, while I wait for the mesa3 to come from production. I have only built a single mesa2 and it is my main keyboard.
- There is a possibility that we will need to add DMA support to the sk2612 driver in Zephyr. Last time I used zephyr on a keyboard, there was no DMA support for the PIO driver and the LEDs would occasionally flicker if an interrupt caused the PIO to underrun.
- The initial goal is to make Dosh more available. The main reason for the bespoke Rust keyboard firmware was for the steno support. With that out of the picture, just adding needed functionality to ZMK makes a lot more sense.
- The longer-term goal is to make a future split, wireless design more practical.
