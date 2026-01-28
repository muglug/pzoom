<?php
$foo = ["a", "b", "c"];
foreach ($foo as $bar) {}
unset($foo, $bar);

function foo(): void {
    $foo = ["a", "b", "c"];
    foreach ($foo as $bar) {}
    unset($foo, $bar);
}