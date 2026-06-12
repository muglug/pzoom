<?php
/** @param numeric-string $s */
function foo(string $s) : void {
    $s = $s + 1;
    if (is_int($s)) {}
}
