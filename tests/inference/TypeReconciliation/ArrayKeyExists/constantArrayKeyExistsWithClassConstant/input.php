<?php
class Foo {
    public const F = "key";
}

/** @param array{key?: string} $a */
function one(array $a): void {
    if (array_key_exists(Foo::F, $a)) {
        echo $a[Foo::F];
    }
}