<?php
class A {
   public ?string $a = null;
   public ?string $b = null;
}

function f(A $obj): string {
    return match (true) {
        $obj->a !== null => $obj->a,
        $obj->b !== null => $obj->b,
        default => throw new \InvalidArgumentException("$obj->a or $obj->b must be set"),
    };
}
