<?php
enum Foo
{
    case Foo;
    case Bar;
}

/** @param value-of<Foo> $arg */
function foobar(string $arg): void {}
                
