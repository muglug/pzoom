<?php
/** @param string[] $arr */
function foo(array $arr): string {
    return current($arr);
}
/** @param string[] $arr */
function bar(array $arr): string {
    $a = current($arr);
    if ($a === false) {
        return "hello";
    }
    return $a;
}
/**
 * @param string[] $arr
 * @return false|string
 */
function bat(array $arr) {
    return current($arr);
}
