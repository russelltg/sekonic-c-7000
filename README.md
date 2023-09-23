# Sekonic C-7000

I wanted Linux drivers for this, so I reverse engineered it

## Setup

copy `60-sekonic.rules` into `/etc/udev/rules.d` to setup permissions for the devices, then just `cargo run`

It's a bit buggy right now, so if it hangs up, restart your C-3000. Oops.

