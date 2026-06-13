<?php
class Foo {
    const BAR = "bar";
    const ONE = 1;
}

/**
 * @param array<string,string> $data
 */
function bat(array $data) : void {
    if (!empty($data["foo"])) {
        if (empty($data[Foo::BAR])) {}
    }
}

/**
 * @param array<int,string> $data
 */
function baz(array $data) : void {
    if (!empty($data[0])) {
        if (empty($data[Foo::ONE])) {}
    }
}
