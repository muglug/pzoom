<?php
/** @param string $s */
function f(?string $s): string {
    if ($s === null) {
        return "";
    }
    return $s;
}

/** @param string $t */
function g(string $t = null): string {
    if ($t === null) {
        return "";
    }
    return $t;
}
