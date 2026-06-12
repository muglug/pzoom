<?php
/** @param non-empty-lowercase-string $num */
function foo(string $num): void {}

/** @param mixed $m */
function bar($m): void
{
    if (is_string($m) && ctype_lower($m)) {
        foo($m);
    }
}
