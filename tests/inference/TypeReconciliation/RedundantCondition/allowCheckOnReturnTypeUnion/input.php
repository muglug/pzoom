<?php
/** @return int|string */
function returnsInt() {
    return rand(0, 1) ? 1 : "hello";
}

if (is_int(returnsInt())) {}
if (!is_int(returnsInt())) {}