<?php
/** @param object $arg */
function test($arg): ?string
{
    /** @psalm-suppress UndefinedClass */
    if ($arg instanceof SomeClassThatDoesNotExist) {
        return null;
    }

    return "b";
}
