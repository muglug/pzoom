<?php
/**
 * @psalm-type Foo      =     string
 * @psalm-type Bar           int
 */
class A {
    /**
     * @psalm-param Foo $foo
     * @psalm-param Bar $bar
     */
    public function bar(string $foo, int $bar): void {}
}
