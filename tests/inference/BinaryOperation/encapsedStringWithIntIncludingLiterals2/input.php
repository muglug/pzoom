<?php
/**
 * @param  literal-int $s1
 * @return literal-string
 */
function foo(int $s1): string {
    $s2 = "foo";
    return "Hello $s1 $s2";
}
