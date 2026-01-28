<?php
/** @return void */
function fooFoo(string $a) {
    if (is_numeric($a)) { }

    if (is_numeric($a) && $a === "1") { }
}

$b = rand(0, 1) ? 5 : false;
if (is_numeric($b)) { }