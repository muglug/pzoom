<?php
/**
 * @psalm-pure
 */
function foo(string &$a): string {
    $a = "B";
    return $a;
}
