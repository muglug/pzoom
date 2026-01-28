<?php
/** @param string[] $arr */
function foo(array $arr) : string {
    return $arr["b"] ?? "bar";
}