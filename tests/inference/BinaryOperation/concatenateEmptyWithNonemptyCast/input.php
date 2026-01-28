<?php
class A
{
    /** @psalm-return non-empty-lowercase-string */
    public function __toString(): string
    {
        return "foo";
    }
}

/** @param non-empty-lowercase-string $arg */
function foo($arg): string
{
    return $arg;
}

$bar = new A();
foo("" . $bar);
