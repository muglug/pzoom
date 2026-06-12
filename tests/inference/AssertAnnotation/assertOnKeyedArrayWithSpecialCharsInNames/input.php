<?php

class Foo {
    /** @var array<string, int> */
    public array $bar;

    /**
     * @param array<string, int> $bar
     */
    public function __construct(array $bar) {
        $this->bar = $bar;
    }
}

$expected = [
    "#[]" => 21,
    "<<>>" => 6,
];

$foo = new Foo($expected);
assertSame($expected, $foo->bar);

/**
 * @psalm-template ExpectedType
 * @psalm-param ExpectedType $expected
 * @psalm-param mixed $actual
 * @psalm-assert =ExpectedType $actual
 */
function assertSame($expected, $actual): void {
    if ($expected !== $actual) {
        throw new Exception("Expected doesn't match actual");
    }
}
