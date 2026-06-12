<?php
abstract class Atomic2 {}
class Lit2 extends Atomic2 {
    public string $value = '';
}

function show(Lit2 $a, Lit2 $b): string {
    $key_values = [$a, $b];
    return implode('|', array_map(static fn(Atomic2 $atomic_type)
        => $atomic_type->value, $key_values));
}
