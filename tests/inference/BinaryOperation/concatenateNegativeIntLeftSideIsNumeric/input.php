<?php
/**
 * @param numeric-string $bar
 * @return int
 */
function foo(string $bar): int
{
    return (int) $bar;
}

foo(foo("-123") . 456);
