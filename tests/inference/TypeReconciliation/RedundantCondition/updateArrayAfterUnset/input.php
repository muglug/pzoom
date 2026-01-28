<?php
/**
 * @param string[] $arr
 */
function foo(string $s) : void {
    $dict = ["a" => 1];
    unset($dict[$s]);
    if (count($dict)) {}
}