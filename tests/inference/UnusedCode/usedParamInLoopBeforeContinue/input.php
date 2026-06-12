<?php
final class Foo {}

function takesFoo(Foo $foo1, Foo $foo2): Foo {
    while (rand(0, 1)) {
        echo get_class($foo1);

        if (rand(0, 1)) {
            $foo1 = $foo2;

            continue;
        }
    }

    return $foo1;
}
