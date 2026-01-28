<?php
enum Foo: int
{
    case Foo = 2;
    case Bar = 3;
}

enum Bar: string
{
    case Foo = "foo";
    case Bar = "bar";
}

/** @param value-of<Foo|Bar> $arg */
function foobar(int|string $arg): void
{
    /** @psalm-check-type-exact $arg = 2|3|"foo"|"bar" */;
}
                
