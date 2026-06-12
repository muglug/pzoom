<?php
function foo(string $s) : void {
    $a = $GLOBALS["s"] ?: [];
    if (count($a)) {}
    if (!count($a)) {}
}
