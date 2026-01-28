<?php
/**
 * @psalm-consistent-constructor
 */
abstract class A {}

function foo(string $a_class) : void {
    if (is_a($a_class, A::class, true)) {
        new $a_class();
    }
}
