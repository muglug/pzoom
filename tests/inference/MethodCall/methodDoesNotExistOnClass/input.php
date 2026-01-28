<?php
class A {}

/**
 * @param class-string<A> $foo
 */
function foo(string $foo): string {
    if (!method_exists($foo, "something")) {
        return "";
    }

    return $foo;
}
