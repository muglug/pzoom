<?php
class A {
    public ?string $name = null;
}

function foo(int $i) : void {
    /** @var array<int, A> */
    $tokens = [];

    if (isset($tokens[$i]->name) && $tokens[$i]->name === "hello") {}
}