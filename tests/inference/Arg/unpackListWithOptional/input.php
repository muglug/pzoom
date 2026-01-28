<?php
function foo(string ...$rest):void {}

$rest = ["zzz"];

if (rand(0,1)) {
    $rest[] = "xxx";
}

foo("first", ...$rest);
