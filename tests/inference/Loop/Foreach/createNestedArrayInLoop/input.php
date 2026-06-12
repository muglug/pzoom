<?php
function foo() : void {
    $arr = [];

    foreach ([1, 2, 3] as $i) {
        $arr[$i]["a"] ??= 0;

        $arr[$i]["a"] += 5;
    }
}
