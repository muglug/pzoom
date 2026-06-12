<?php
/** @param numeric-string $num */
function foo(string $num): void {}

/** @param mixed $m */
function bar(mixed $m): void
{
    if (is_string($m) && ctype_digit($m)) {
        foo($m);
    }
}
