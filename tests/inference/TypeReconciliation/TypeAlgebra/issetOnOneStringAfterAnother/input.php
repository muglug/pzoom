<?php
/** @param string[] $arr */
function foo(array $arr): void {
    $a = "a";

    if (!isset($arr[$a])) {
        return;
    }

    foreach ([0, 1, 2, 3] as $i) {
        if (!isset($arr[$a . $i])) {
            echo "a";
        }

        $a = "hello";
    }
}