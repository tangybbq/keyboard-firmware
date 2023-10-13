# David Brown's Keyboard Firmware

This is the firmware for my various
[keyboards](https://github.com/tangybbq/keyboard).  At this point, it isn't the
firmware I regularly run, but is still in development.  It is based on the
[rp2040 project template](https://github.com/rp-rs/rp2040-project-template).

At this point, there is nothing to configure, as it only supports the proto2
keyboard.

For the intra-keyboard cable, it is necessary to reverse the cable (meaning pin
1 on one side connects to pin 5 on the other, and vice-versa). This is necessary
because the rp2040 UART has specific pins it can be connected to. A future
design will do the connector differently so this is not an issue.

At this point, the two halves work properly and the strokes are sent, as in
steno mode, in raw mode, followed by a space.

The LEDs try to indicate status, but don't always work correctly. I'll debug
this after I get more functionality working.

I was able to get probe-rs working with the picoprobe by using [this
firmware](https://github.com/raspberrypi/picoprobe/releases/download/picoprobe-cmsis-v1.02/picoprobe.uf2).
