<?php
class Foo
{
    /** @var int */
    public $a = 0;
}

function accessOnVar(?Foo $bar, string $b) : void {
    /** @psalm-suppress MixedArgument */
    echo $bar->{$b} ?? null;
}
