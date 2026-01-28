<?php
/** @param array<int, string> $arr */
function foo(array $arr) : void {
    if (array_key_exists(1, $arr)) {
        $a = ($arr[1] === "b") ? true : false;
    }
}