<?php
class A {}
class B {}

/** @param class-string $class */
function foo(string $class) : void {
    if ($class === A::class) {}
    if ($class === A::class || $class === B::class) {}
}
