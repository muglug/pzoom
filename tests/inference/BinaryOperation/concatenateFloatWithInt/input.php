<?php
/**
 * @param numeric-string $bar
 * @return numeric-string
 */
function foo(string $bar): string
{
    return $bar;
}

foo(-123.456 . 789);
