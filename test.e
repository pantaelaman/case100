start
  cpdata 0x80000062 0
  cpdata 0x80000063 0
  cpdata 0x80000064 639
  cpdata 0x80000065 479
  cpdata 0x80000066 0xffffffff
  cpdata 0x80000060 1

  cpdata 0x80000020 1
kbd_wait_loop
  be kbd_wait_loop 0x80000020 one
  cp kbd_pressed 0x80000021
  cp kbd_key 0x80000022
  halt

one 1
kbd_pressed 0
kbd_key 0
