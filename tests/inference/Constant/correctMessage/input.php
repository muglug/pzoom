<?php
class S {
    public const ZERO = 0;
    public const ONE  = 1;
}

/**
 * @param S::* $s
 */
function foo(int $s): string {
    return [1 => "a", 2 => "b"][$s];
}
