<?php
function Foo(string $a, string ...$b) : void {}

/** @return array<array-key, string> */
function Baz(string ...$c) {
    Foo(...$c);
    return $c;
}
