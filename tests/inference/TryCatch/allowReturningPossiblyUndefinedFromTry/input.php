<?php
function fooFunction (): string {
    try{
        $foo = "foo";
    } finally {
        /** @psalm-suppress UndefinedVariable */
        echo $foo;
    }

    return $foo;
}
