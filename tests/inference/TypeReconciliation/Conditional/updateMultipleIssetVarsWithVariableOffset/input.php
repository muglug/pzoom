<?php
/** @return void **/
function foo(string $s) {}

$a = rand(0, 1) ? ["hello"] : null;
$b = 0;
if (isset($a[$b])) {
    foo($a[$b]);
}