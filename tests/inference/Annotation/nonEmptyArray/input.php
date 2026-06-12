<?php
/** @param non-empty-array<string> $arr */
function foo(array $arr) : void {
    foreach ($arr as $a) {}
    echo $a;
}

foo(["a", "b", "c"]);

/** @param array<string> $arr */
function bar(array $arr) : void {
    if (!$arr) {
        return;
    }

    foo($arr);
}
