<?php

interface FooInterface
{
    public function toString(): ?string;
}

function example(object $foo): string
{
    return ($foo instanceof FooInterface ? $foo->toString() : null) ?? "Not a stringable foo";
}
