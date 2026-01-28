<?php
class A {}

function getAOrFalse(bool $b) : A|false {
    return false;
}

function foo(A|false $a): void
{
    if ($a instanceof A
        || ($a = getAOrFalse($a))
    ) {
    }
}