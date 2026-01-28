<?php
class A
{
    private const IS_PRIVATE = 1;

    function fooFoo(): int {
        return A::IS_PRIVATE;
    }
}
