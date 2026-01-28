<?php
/**
 * @param scalar|null $val
 */
function foo($val) : ? bool {
    if ("1" === $val || 1 === $val) {
        return true;
    } elseif ("0" === $val || 0 === $val) {
        return false;
    }

    return null;
}