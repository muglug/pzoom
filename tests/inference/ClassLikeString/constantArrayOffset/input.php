<?php
class A {
    const FOO = [
        B::class => "bar",
    ];
}
class B {}

/** @param class-string $s */
function bar(string $s) : void {}

foreach (A::FOO as $class => $_) {
    bar($class);
}
