<?php
/** @return void **/
function foo(string $s) {}

$a = rand(0, 1) ? ["hello"] : null;
if (isset($a[0])) {
    foo($a[0]);
}