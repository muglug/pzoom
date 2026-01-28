<?php
function fooFunction (): string {
    try{
        $foo = "foo";
    } finally {
        /** @psalm-suppress UndefinedVariable */
        echo $foo;
        $foo = "bar";
    }

    return $foo;
}
