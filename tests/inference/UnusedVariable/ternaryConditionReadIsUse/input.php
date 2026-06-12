<?php
/** @return array<string, int>|null */
function f(int $i): ?array
{
    $return_null = false;
    $defaults = ['a' => 1];

    if ($i > 5) {
        if ($i > 10) {
            $defaults['b'] = 2;
        } else {
            $return_null = true;
        }
    }

    return $return_null ? null : $defaults;
}
