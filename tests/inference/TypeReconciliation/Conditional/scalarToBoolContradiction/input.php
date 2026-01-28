<?php
/** @param mixed $s */
function foo($s) : void {
    if (!is_scalar($s)) {
        return;
    }

    if (!is_bool($s)) {
        if (is_bool($s)) {}
    }
}
