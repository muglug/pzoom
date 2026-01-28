<?php
/**
 * @param string[] $arr
 */
function foo(array $arr) : void {
    $dict = ["a" => 1, "b" => 2, "c" => 3];

    foreach ($arr as $v) {
        unset($dict[$v]);
    }

    if (count($dict)) {}
}