<?php
$ffi = FFI::cdef(
    "int printf(const char *format, ...);", // this is a regular C declaration
    "libc.so.6"
);
$ffi->printf("Hello %s!\n", "world");
                
