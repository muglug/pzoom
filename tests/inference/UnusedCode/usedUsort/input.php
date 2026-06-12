<?php
/** @param string[] $arr */
function foo(array $arr) : array {
    usort($arr, "strnatcasecmp");
    return $arr;
}
