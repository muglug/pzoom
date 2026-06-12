<?php
function fooFunction (): string {
    try{
        $foo = "foo";
    } finally {
        /** @psalm-suppress PossiblyUndefinedVariable */
        echo $foo;
        $foo = "bar";
    }

    return $foo;
}
