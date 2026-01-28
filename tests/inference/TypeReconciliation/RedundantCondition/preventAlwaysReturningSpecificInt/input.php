<?php
/**
 * @return 3|4
 */
function returnsInt(): int {
    return rand(0, 1) ? 3 : 4;
}

if (is_int(returnsInt())) {}
