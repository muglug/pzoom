<?php
/**
 * @psalm-type Foo=array{
 *   a: string, // comment
 *   b: string, // comment
 * }
 */
class A {
    /**
     * @psalm-param Foo $foo
     */
    public function bar(array $foo): void {}
}
