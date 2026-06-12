<?php

function f(bool $cond): string
{
    $x = null;
    if ($cond) {
        $x = 'set';
    }

    $x ??= 'fallback';

    return $x;
}
