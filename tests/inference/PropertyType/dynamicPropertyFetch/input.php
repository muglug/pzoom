<?php
class Foo {
    public int $a = 0;
}

function takesFoo(?Foo $foo, string $b) : void {
    /** @psalm-suppress MixedArgument */
    echo $foo->{$b} ?? null;
}
