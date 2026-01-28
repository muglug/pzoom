<?php
/** @param non-empty-array<string> $arr */
function foo(array $arr) : void {
    foreach ($arr as $a) {}
    echo $a;
}

foo([]);
