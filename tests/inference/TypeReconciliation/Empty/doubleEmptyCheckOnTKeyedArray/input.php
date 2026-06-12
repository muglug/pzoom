<?php
/**
 * @param array{a: array, b: array} $arr
 */
function foo(array $arr) : void {
    if (empty($arr["a"]) && empty($arr["b"])) {}
}
