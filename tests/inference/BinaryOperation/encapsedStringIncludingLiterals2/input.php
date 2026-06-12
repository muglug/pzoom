<?php
/**
 * @param  literal-string $s1
 * @return literal-string
 */
function foo(string $s1): string {
    $s2 = 2;
    return "Hello $s1 $s2";
}
