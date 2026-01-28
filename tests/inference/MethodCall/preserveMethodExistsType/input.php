<?php
/**
 * @param class-string $foo
 */
function foo(string $foo): string {
    if (!method_exists($foo, "something")) {
        return "";
    }

    return $foo;
}
