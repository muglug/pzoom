<?php
$ffi = FFI::cdef(
    "int errno;", // this is a regular C declaration
    "libc.so.6"
);
echo $ffi->errno;
                
