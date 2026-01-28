<?php
class A {
    public ?string $name = null;
}

function foo(int $i) : void {
    /** @var array<int, A> */
    $tokens = [];

    if (!isset($tokens[$i])) {
        if (rand(0, 1)) {
            if (rand(0, 1)) {
                $tokens[$i] = new A();
            } else {
                return;
            }
        } else {
            return;
        }
    }

    echo $tokens[$i]->name;
}