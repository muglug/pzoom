<?php

class TT9 {
    /** @var array<string, string> */
    public array $extra_types = [];
}

function f(TT9 $atomic_type): ?string
{
    if ($atomic_type->extra_types) {
        $first = array_shift($atomic_type->extra_types);
        assert($first !== null);

        if ($atomic_type->extra_types) {
            return $first . 'more';
        }
        return $first;
    }
    return null;
}
