<?php
/** @var string */
$a = 0;
function foo(): void
{
    $b = 1;
    $a = &$b;
    global $a;
    takesString($a);
}

function takesString(string $str): void {}
