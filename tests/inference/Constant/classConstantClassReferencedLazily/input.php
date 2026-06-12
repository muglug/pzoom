<?php
/** @return array<string, int> */
function getMap(): array {
    return Mapper::MAP;
}

class Mapper {
    public const MAP = [
        Foo::class => self::A,
        Foo::BAR => self::A,
    ];

    private const A = 5;
}

class Foo {
    public const BAR = "bar";
}
