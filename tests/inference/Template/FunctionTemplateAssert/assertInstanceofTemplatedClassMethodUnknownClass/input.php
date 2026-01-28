<?php
namespace Bar;

class C {
    /**
     * @template T as object
     * @param class-string<T> $expected
     * @param mixed  $actual
     * @psalm-assert T $actual
     */
    public function assertInstanceOf($expected, $actual) : void {}

    /**
     * @param class-string $c
     */
    function bar(string $c, object $e) : void {
        $this->assertInstanceOf($c, $e);
        echo $e->getCode();
    }
}