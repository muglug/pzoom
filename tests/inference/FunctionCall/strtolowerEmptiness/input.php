<?php
/** @param non-empty-string $s */
function foo(string $s) : void {
    $s = strtolower($s);

    foo($s);
}
